extern crate image;

use rayon::prelude::*;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(name = "subtitle_tonemap", about = "Tonemap PGS Subtitles")]
struct Opt {
    #[structopt(short = "-p", long, default_value = "60")]
    percentage: f32,

    #[structopt(short = "-f", long)]
    fixed: bool,

    #[structopt(short = "-o", long, parse(from_os_str))]
    output: PathBuf,

    #[structopt(name = "input", parse(from_os_str))]
    input: PathBuf,
}

fn main() -> std::io::Result<()> {
    let now = Instant::now();
    let opt = Opt::from_args();

    let mut working_dir = env::current_dir()?;

    // Make sure jar file exists in the same directory
    let mut java_jar = env::current_exe()?;
    java_jar.pop();
    java_jar.push("BDSup2Sub512.jar");
    assert!(
        java_jar.exists(),
        "BDSup2Sub should be in the same directory as this executable."
    );

    let input = opt.input;
    let output = opt.output;

    assert!(
        opt.percentage <= 100.0,
        "Percentage has to be between 0 and 100."
    );
    let percentage: f32 = opt.percentage / 100.0;
    let fixed: bool = opt.fixed;

    working_dir.push(output.as_path());

    // Create output dir if it doesn't exist
    if !output.exists() {
        fs::create_dir(output)?;
    }

    let mut files: Vec<PathBuf> = Vec::new();
    if input.exists() {
        if input.is_dir() {
            files.extend(
                input
                    .read_dir()
                    .expect("Couldn't read directory content")
                    .filter_map(Result::ok)
                    .filter(|e| e.metadata().expect("Couldn't get file metadata").is_file())
                    .filter(|e| e.path().extension().expect("File has no extension") == "sup")
                    .map(|e| {
                        let mut path = PathBuf::from(&working_dir);
                        path.pop();
                        path.push(e.path());

                        path
                    }),
            );
        } else if input.extension().expect("File has no extension") == "sup" {
            files.push(input);
        }
    }

    let total: u64 = files.len() as u64;

    (0..files.len()).into_par_iter().for_each(|current| {
        let file = &files[current];

        println!("Tonemapping subtitle #{} of {}", current + 1, total);
        extract_images(&java_jar, &working_dir, &file, current)
            .and_then(|out_file| process_images(out_file, percentage, fixed))
            .and_then(|timestamps| merge_images(&java_jar, &working_dir, &file, timestamps))
            .and_then(cleanup_images)
            .ok();
    });

    println!("Done: {:#?} elapsed", now.elapsed());

    Ok(())
}

fn extract_images(
    java_jar: &PathBuf,
    working_dir: &PathBuf,
    file: &PathBuf,
    index: usize,
) -> Result<PathBuf, std::io::Error> {
    let mut out_file = PathBuf::from(working_dir);
    out_file.push(format!("sub{}", index));

    if !out_file.exists() {
        fs::create_dir(&out_file)?;
    }

    out_file.push(format!("sub{}.xml", index));

    let output = Command::new("java")
        .args(&[
            "-jar",
            java_jar.to_str().unwrap(),
            "-T",
            "keep",
            "-o",
            out_file.to_str().unwrap(),
            file.to_str().unwrap(),
        ])
        .output()
        .expect("Failed to execute process");

    if output.status.success() {
        Ok(out_file)
    } else {
        panic!("Couldn't run imagex extraction");
    }
}

fn process_images(file: PathBuf, percentage: f32, fixed: bool) -> Result<PathBuf, std::io::Error> {
    let mut in_dir = PathBuf::from(&file);
    in_dir.pop();

    let images: Vec<PathBuf> = in_dir
        .read_dir()
        .expect("Couldn't read images directory")
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().expect("File has no extension") == "png")
        .map(|e| e.path())
        .collect();

    images
        .par_iter()
        .map(|i| {
            let mut img = image::open(&i).expect("Opening image failed").to_rgba();

            img.pixels_mut()
                .filter(|p| {
                    let image::Rgba(data) = **p;
                    if fixed {
                        (data[0] > 100 && data[1] > 100 && data[2] > 100 && data[3] > 0)
                    } else {
                        (data[0] > 1 && data[1] > 1 && data[2] > 1 && data[3] > 0)
                    }
                })
                .for_each(|p| {
                    let image::Rgba(mut data) = *p;
                    
                    if fixed {
                        data[0] = (255.0 * percentage).round() as u8;
                        data[1] = (255.0 * percentage).round() as u8;
                        data[2] = (255.0 * percentage).round() as u8;
                    } else {
                        data[0] = (f32::from(data[0]) * percentage).round() as u8;
                        data[1] = (f32::from(data[1]) * percentage).round() as u8;
                        data[2] = (f32::from(data[2]) * percentage).round() as u8;
                    }

                    *p = image::Rgba(data);
                });

            (i, img)
        })
        .for_each(|(path, img)| img.save(&path).unwrap());

    Ok(file)
}

fn merge_images(
    java_jar: &PathBuf,
    working_dir: &PathBuf,
    file: &PathBuf,
    timestamps: PathBuf,
) -> Result<PathBuf, std::io::Error> {
    let mut out_file = PathBuf::from(&working_dir);
    out_file.push(file.file_name().unwrap());

    let output = Command::new("java")
        .args(&[
            "-jar",
            java_jar.to_str().unwrap(),
            "-T",
            "keep",
            "-o",
            out_file.to_str().unwrap(),
            timestamps.to_str().unwrap(),
        ])
        .output()
        .expect("Failed to execute process");

    if output.status.success() {
        Ok(timestamps)
    } else {
        panic!("Couldn't run imagex extraction");
    }
}

fn cleanup_images(dir: PathBuf) -> Result<PathBuf, std::io::Error> {
    let mut dir_to_rm = PathBuf::from(&dir);
    dir_to_rm.pop();

    fs::remove_dir_all(dir_to_rm)?;

    Ok(dir)
}
