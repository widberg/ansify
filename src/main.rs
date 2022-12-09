use ansi_term::Colour::Fixed;
use clap::Parser;
use image::io::Reader as ImageReader;
use image::RgbImage;
use kd_tree::KdMap;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::File;
use std::path::PathBuf;
use std::vec::Vec;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[arg(short, long, value_name = "INPUT_PATH")]
    input: PathBuf,

    #[arg(short, long, value_name = "OUTPUT_PATH")]
    output: Option<PathBuf>,

    #[arg(short, long, value_name = "PALETTE_PATH")]
    palette: PathBuf,

    #[arg(short, long, value_name = "BLOCKS_PATH")]
    blocks: PathBuf,

    #[arg(short, long, value_name = "WIDTH")]
    width: Option<u32>,

    #[arg(short = 'H', long, value_name = "HEIGHT")]
    height: Option<u32>,

    #[arg(short, long)]
    text: bool,

    #[arg(short, long)]
    verbose: bool,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Palette {
    colors: Vec<[u8; 3]>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Blocks {
    width: u32,
    height: u32,
    blocks: BTreeMap<char, Vec<Vec<bool>>>,
}

struct Shade {
    ratio: f32,
    block: char,
}

struct Texel {
    foreground_color: u8,
    background_color: u8,
    block: char,
}

fn count_foreground_pixels(bitmap: &Vec<Vec<bool>>) -> u32 {
    return bitmap
        .into_iter()
        .flat_map(IntoIterator::into_iter)
        .map(|x| *x as u32)
        .sum();
}

fn blend_two_colors(color_a: &[f32; 3], color_b: &[f32; 3], ratio: f32) -> [f32; 3] {
    return [
        color_a[0] * ratio + color_b[0] * (1.0 - ratio),
        color_a[1] * ratio + color_b[1] * (1.0 - ratio),
        color_a[2] * ratio + color_b[2] * (1.0 - ratio),
    ];
}

fn normalize_color(color: &[u8; 3]) -> [f32; 3] {
    return [
        color[0] as f32 / 255.0,
        color[1] as f32 / 255.0,
        color[2] as f32 / 255.0,
    ];
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    if cli.verbose {
        println!("Opening and parsing palette");
    }

    let file = File::open(cli.palette)?;
    let palette: Palette = serde_yaml::from_reader(&file)?;

    if cli.verbose {
        println!("Opening and parsing blocks");
    }

    let file2 = File::open(cli.blocks)?;
    let blocks: Blocks = serde_yaml::from_reader(&file2)?;

    if cli.verbose {
        println!("Verifying block dimensions");
    }

    for (_character, bitmap) in blocks.blocks.iter() {
        assert!(bitmap.len() == blocks.height as usize);
        for row in bitmap {
            assert!(row.len() == blocks.width as usize);
        }
    }

    if cli.verbose {
        println!("Opening original image");
    }

    let original_image = ImageReader::open(cli.input)?.decode()?;

    if cli.verbose {
        println!("Calculating dimension and resizing");
    }

    let ratio = (original_image.width() as f32 / blocks.width as f32)
        / (original_image.height() as f32 / blocks.height as f32);

    let img = match (cli.width, cli.height) {
        (None, None) => original_image,
        (Some(width), None) => original_image.resize_exact(
            width,
            (width as f32 / ratio) as u32,
            image::imageops::Lanczos3,
        ),
        (None, Some(height)) => original_image.resize_exact(
            (height as f32 * ratio) as u32,
            height,
            image::imageops::Lanczos3,
        ),
        (Some(width), Some(height)) => {
            original_image.resize_exact(width, height, image::imageops::Lanczos3)
        }
    }
    .into_rgb8();

    if cli.verbose {
        println!("Generating shades");
    }

    let mut shades = Vec::new();
    for (character, bitmap) in blocks.blocks.iter() {
        shades.push(Shade {
            ratio: count_foreground_pixels(bitmap) as f32 / (blocks.width * blocks.height) as f32,
            block: *character,
        });
    }

    if cli.verbose {
        println!("Generating texels");
    }

    let mut texels = Vec::new();

    for shade in shades.iter() {
        if ratio == 0.0 {
            for (i, color) in palette.colors.iter().enumerate() {
                texels.push((
                    normalize_color(color),
                    Texel {
                        foreground_color: 0 as u8,
                        background_color: i as u8,
                        block: shade.block,
                    },
                ));
            }
        } else if ratio == 1.0 {
            for (i, color) in palette.colors.iter().enumerate() {
                texels.push((
                    normalize_color(color),
                    Texel {
                        foreground_color: i as u8,
                        background_color: 0 as u8,
                        block: shade.block,
                    },
                ));
            }
        } else {
            for (i, foreground_color) in palette.colors.iter().enumerate() {
                for (j, background_color) in palette.colors.iter().enumerate() {
                    if foreground_color == background_color {
                        continue;
                    }
                    let color = blend_two_colors(
                        &normalize_color(foreground_color),
                        &normalize_color(background_color),
                        shade.ratio,
                    );
                    texels.push((
                        color,
                        Texel {
                            foreground_color: i as u8,
                            background_color: j as u8,
                            block: shade.block,
                        },
                    ));
                }
            }
        }
    }

    if cli.verbose {
        println!("Generate kdtree");
    }

    let kdtree = KdMap::par_build_by_ordered_float(texels);

    if cli.verbose {
        println!("Creating output image");
    }

    let mut out = RgbImage::new(img.width() * blocks.width, img.height() * blocks.height);

    if cli.verbose {
        println!("Generating output");
    }

    for (x, y, pixel) in img.enumerate_pixels() {
        let nearest = kdtree
            .nearest(&[
                pixel.0[0] as f32 / 255.0,
                pixel.0[1] as f32 / 255.0,
                pixel.0[2] as f32 / 255.0,
            ])
            .unwrap()
            .item;
        let texel = &nearest.1;
        if cli.text {
            print!(
                "{}",
                Fixed(texel.foreground_color)
                    .on(Fixed(texel.background_color))
                    .paint(texel.block.to_string())
            );
            if x + 1 == img.width() {
                println!("");
            }
        }
        for i in 0..blocks.width {
            for j in 0..blocks.height {
                out.put_pixel(
                    x * blocks.width + i,
                    y * blocks.height + j,
                    if blocks.blocks[&texel.block][j as usize][i as usize] {
                        let foreground_color = palette.colors[texel.foreground_color as usize];
                        image::Rgb {
                            0: foreground_color,
                        }
                    } else {
                        let background_color = palette.colors[texel.background_color as usize];
                        image::Rgb {
                            0: background_color,
                        }
                    },
                );
            }
        }
    }

    if let Some(output_path) = cli.output {
        if cli.verbose {
            println!("Writing output");
        }

        out.save(output_path)?;
    }

    if cli.verbose {
        println!("Done");
    }

    return Ok(());
}
