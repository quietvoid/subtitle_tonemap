## subtitle_tonemap
Maps PGS subtitles to a different color/brightness

## Requirements

* `BDSup2Sub` https://www.videohelp.com/download/BDSup2Sub512.jar
* `Java runtime`

## Options
* `<INPUT>` Input subtitle file or directory containing PGS subtitles. Positional argument.
* `--output`, `-o` Output directory.
* `--percentage`, `-p` Percentage to multiply the final color of the subtitle. Defaults to 60%.
* `--fixed`, `-f` Use 100% white as base color instead of the subtitle's original color.
* `--color`, `-c` Hexadecimal color value to use as base color for `--fixed`. RRGGBB.
    - Overrides `--fixed` to true when set.
### Usage, in CLI:

* BDSup2Sub512.jar has to be in the same directory as the executable.
* `subtitle_tonemap.exe "path/to/subtitles" -o tonemapped`

Will tonemap the input subtitles (can be a single file or directory input) to the output directory.
