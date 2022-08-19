use std::env;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

use clap::Parser;
use clap::ValueHint;
use rayon::prelude::*;

#[derive(Parser, Debug)]
#[clap(name = env!("CARGO_PKG_NAME"), about = "Maps PGS subtitles to a different color/brightness", author = "quietvoid", version = env!("CARGO_PKG_VERSION"))]
struct Opt {
    #[clap(
        name = "input",
        help = "Input subtitle file or directory containing PGS subtitles",
        value_hint = ValueHint::FilePath
    )]
    input: PathBuf,

    #[clap(
        short = 'o',
        long,
        help = "Output directory",
        value_hint = ValueHint::FilePath
    )]
    output: PathBuf,

    #[clap(
        short = 'p',
        long,
        default_value = "60",
        help = "Percentage to multiply the final color of the subtitle"
    )]
    percentage: f32,

    #[clap(
        short = 'f',
        long,
        help = "Use 100% white as base color instead of the subtitle's original color"
    )]
    fixed: bool,

    #[clap(
        short = 'c',
        long,
        help = "Hexadecimal color value to use as base color for --fixed. RRGGBB"
    )]
    color: Option<String>,
}

fn main() -> std::io::Result<()> {
    let now = Instant::now();
    let opt = Opt::parse();

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

    let ratio: f32 = opt.percentage / 100.0;
    let mut fixed: bool = opt.fixed;

    let color = if let Some(c) = opt.color {
        fixed = true;

        assert_eq!(c.len(), 6);

        let rr = u8::from_str_radix(&c[0..2], 16).unwrap_or(255);
        let gg = u8::from_str_radix(&c[2..4], 16).unwrap_or(255);
        let bb = u8::from_str_radix(&c[4..6], 16).unwrap_or(255);

        vec![rr as f32, gg as f32, bb as f32]
    } else {
        vec![255.0, 255.0, 255.0]
    };

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
        extract_images(&java_jar, &working_dir, file, current)
            .and_then(|out_file| process_images(out_file, ratio, fixed, &color))
            .and_then(|timestamps| merge_images(&java_jar, &working_dir, file, timestamps))
            .and_then(cleanup_images)
            .ok();
    });

    println!("Done: {:#?} elapsed", now.elapsed());

    Ok(())
}

fn extract_images(
    java_jar: &Path,
    working_dir: &Path,
    file: &Path,
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

fn process_images(
    file: PathBuf,
    ratio: f32,
    fixed: bool,
    color: &[f32],
) -> Result<PathBuf, std::io::Error> {
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
            let mut img = image::open(&i).expect("Opening image failed").to_rgba8();
            let old_max = img
                .pixels()
                .map(|p| {
                    let image::Rgba(data) = *p;
                    get_lightness(data[0] as f32, data[1] as f32, data[2] as f32)
                })
                .max_by(|x, y| x.abs().partial_cmp(&y.abs()).unwrap())
                .unwrap();

            img.pixels_mut()
                .filter(|p| {
                    let image::Rgba(data) = **p;

                    data[0] > 1 && data[1] > 1 && data[2] > 1 && data[3] > 0
                })
                .for_each(|p| {
                    let image::Rgba(mut data) = *p;

                    if fixed {
                        let src_lightness =
                            get_lightness(data[0] as f32, data[1] as f32, data[2] as f32);

                        let scale = (src_lightness * ratio) / old_max;

                        let r = color[0] * scale;
                        let g = color[1] * scale;
                        let b = color[2] * scale;

                        data[0] = r.round().clamp(0.0, color[0]) as u8;
                        data[1] = g.round().clamp(0.0, color[1]) as u8;
                        data[2] = b.round().clamp(0.0, color[2]) as u8;
                    } else {
                        data[0] = (data[0] as f32 * ratio).round().clamp(0.0, 255.0) as u8;
                        data[1] = (data[1] as f32 * ratio).round().clamp(0.0, 255.0) as u8;
                        data[2] = (data[2] as f32 * ratio).round().clamp(0.0, 255.0) as u8;
                    }

                    *p = image::Rgba(data);
                });

            (i, img)
        })
        .for_each(|(path, img)| img.save(&path).unwrap());

    Ok(file)
}

fn merge_images(
    java_jar: &Path,
    working_dir: &Path,
    file: &Path,
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

#[inline(always)]
fn get_lightness(r: f32, g: f32, b: f32) -> f32 {
    let rp = r / 255.0;
    let gp = g / 255.0;
    let bp = b / 255.0;
    let cmax = rp.max(gp).max(bp);
    let cmin = rp.min(gp).min(bp);

    (cmax + cmin) / 2.0
}
