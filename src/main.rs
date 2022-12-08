use clap::Parser;
use image::io::Reader as ImageReader;
use image::RgbImage;
use kdtree::distance::squared_euclidean;
use kdtree::KdTree;
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
    output: PathBuf,

    #[arg(short, long, value_name = "PALETTE_PATH")]
    palette: PathBuf,

    #[arg(short, long, value_name = "BLOCKS_PATH")]
    blocks: PathBuf,

    #[arg(short, long, value_name = "WIDTH")]
    width: Option<u32>,

    #[arg(short = 'H', long, value_name = "HEIGHT")]
    height: Option<u32>,
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
    let mut result = 0;

    for row in bitmap {
        for bit in row {
            if *bit {
                result += 1;
            }
        }
    }

    return result;
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

    println!("Generating kdtree");

    let file = File::open(cli.palette)?;
    let palette: Palette = serde_yaml::from_reader(&file)?;

    let file2 = File::open(cli.blocks)?;
    let blocks: Blocks = serde_yaml::from_reader(&file2)?;

    for (_character, bitmap) in blocks.blocks.iter() {
        assert!(bitmap.len() == blocks.height as usize);
        for row in bitmap {
            assert!(row.len() == blocks.width as usize);
        }
    }

    let original_image = ImageReader::open(cli.input)?.decode()?.into_rgb8();

    let ratio = (original_image.width() as f32 / blocks.width as f32)
        / (original_image.height() as f32 / blocks.height as f32);

    let img = match (cli.width, cli.height) {
        (None, None) => original_image,
        (Some(width), None) => image::imageops::resize(
            &original_image,
            width,
            (width as f32 / ratio) as u32,
            image::imageops::Lanczos3,
        ),
        (None, Some(height)) => image::imageops::resize(
            &original_image,
            (height as f32 * ratio) as u32,
            height,
            image::imageops::Lanczos3,
        ),
        (Some(width), Some(height)) => {
            image::imageops::resize(&original_image, width, height, image::imageops::Lanczos3)
        }
    };

    let mut shades: Vec<Shade> = Vec::new();
    for (character, bitmap) in blocks.blocks.iter() {
        shades.push(Shade {
            ratio: count_foreground_pixels(bitmap) as f32 / (blocks.width * blocks.height) as f32,
            block: *character,
        });
    }

    let mut kdtree = KdTree::new(3);

    for shade in shades {
        if ratio == 0.0 {
            for (i, color) in palette.colors.iter().enumerate() {
                kdtree.add(
                    normalize_color(color),
                    Texel {
                        foreground_color: 0 as u8,
                        background_color: i as u8,
                        block: shade.block,
                    },
                )?;
            }
        } else if ratio == 1.0 {
            for (i, color) in palette.colors.iter().enumerate() {
                kdtree.add(
                    normalize_color(color),
                    Texel {
                        foreground_color: i as u8,
                        background_color: 0 as u8,
                        block: shade.block,
                    },
                )?;
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
                    kdtree.add(
                        color,
                        Texel {
                            foreground_color: i as u8,
                            background_color: j as u8,
                            block: shade.block,
                        },
                    )?;
                }
            }
        }
    }

    let mut out = RgbImage::new(img.width() * blocks.width, img.height() * blocks.height);

    println!("Generating output");

    for (x, y, pixel) in img.enumerate_pixels() {
        let nearest = kdtree
            .nearest(
                &vec![
                    pixel.0[0] as f32 / 255.0,
                    pixel.0[1] as f32 / 255.0,
                    pixel.0[2] as f32 / 255.0,
                ],
                1,
                &squared_euclidean,
            )
            .unwrap();
        let texel = nearest[0].1;
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

    println!("Done");

    out.write_to(
        &mut File::create(cli.output)?,
        image::ImageOutputFormat::Bmp,
    )?;

    return Ok(());
}
