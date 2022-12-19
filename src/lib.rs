use ansi_term::Colour::Fixed;
use image::RgbImage;
use kd_tree::KdMap;
use log::info;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::File;
use std::path::PathBuf;
use std::vec::Vec;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Palette {
    colors: Vec<[u8; 3]>,
}

impl Palette {
    pub fn from(path: PathBuf) -> Result<Palette, Box<dyn std::error::Error>> {
        info!("Opening and parsing palette");

        let file = File::open(path)?;
        return Ok(serde_yaml::from_reader(&file)?);
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Blocks {
    width: u32,
    height: u32,
    blocks: BTreeMap<char, Vec<Vec<bool>>>,
}

impl Blocks {
    pub fn from(path: PathBuf) -> Result<Blocks, Box<dyn std::error::Error>> {
        info!("Opening and parsing blocks");

        let file2 = File::open(path)?;
        let blocks: Blocks = serde_yaml::from_reader(&file2)?;

        info!("Verifying block dimensions");

        for (_character, bitmap) in blocks.blocks.iter() {
            assert!(bitmap.len() == blocks.height as usize);
            for row in bitmap {
                assert!(row.len() == blocks.width as usize);
            }
        }

        return Ok(blocks);
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }
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

pub struct ANSIfier {
    palette: Palette,
    pub blocks: Blocks,
    kdtree: KdMap<[f32; 3], Texel>,
}

impl ANSIfier {
    pub fn new(palette: Palette, blocks: Blocks) -> ANSIfier {
        info!("Generating shades");

        let mut shades = Vec::new();
        for (character, bitmap) in blocks.blocks.iter() {
            shades.push(Shade {
                ratio: count_foreground_pixels(bitmap) as f32
                    / (blocks.width * blocks.height) as f32,
                block: *character,
            });
        }

        info!("Generating texels");

        let mut texels = Vec::new();

        for shade in shades.iter() {
            if shade.ratio == 0.0 {
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
            } else if shade.ratio == 1.0 {
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

        info!("Generate kdtree");

        return ANSIfier {
            palette,
            blocks,
            #[cfg(feature = "rayon")]
            kdtree: KdMap::par_build_by_ordered_float(texels),
            #[cfg(not(feature = "std"))]
            kdtree: KdMap::build_by_ordered_float(texels),
        };
    }

    pub fn process(&self, img: &RgbImage) -> (RgbImage, String) {
        info!("Creating output image");

        let mut out = RgbImage::new(
            img.width() * self.blocks.width,
            img.height() * self.blocks.height,
        );
        let mut text = String::new();

        info!("Generating output");

        for (x, y, pixel) in img.enumerate_pixels() {
            let nearest = self
                .kdtree
                .nearest(&[
                    pixel.0[0] as f32 / 255.0,
                    pixel.0[1] as f32 / 255.0,
                    pixel.0[2] as f32 / 255.0,
                ])
                .unwrap()
                .item;
            let texel = &nearest.1;
            text.push_str(
                &Fixed(texel.foreground_color)
                    .on(Fixed(texel.background_color))
                    .paint(texel.block.to_string())
                    .to_string(),
            );

            if x + 1 == img.width() {
                text.push('\n');
            }
            let foreground_color = self.palette.colors[texel.foreground_color as usize];
            let background_color = self.palette.colors[texel.background_color as usize];
            for i in 0..self.blocks.width {
                for j in 0..self.blocks.height {
                    out.put_pixel(
                        x * self.blocks.width + i,
                        y * self.blocks.height + j,
                        image::Rgb {
                            0: if self.blocks.blocks[&texel.block][j as usize][i as usize] {
                                foreground_color
                            } else {
                                background_color
                            },
                        },
                    );
                }
            }
        }

        return (out, text);
    }

    pub fn calculate_new_dimensions(
        &self,
        original_dimensions: (u32, u32),
        desired_dimensions: (Option<u32>, Option<u32>),
    ) -> (u32, u32) {
        info!("Calculating dimension and resizing");

        let ratio = (original_dimensions.0 as f32 / self.block_width() as f32)
            / (original_dimensions.1 as f32 / self.block_height() as f32);

        return match desired_dimensions {
            (None, None) => original_dimensions,
            (Some(width), None) => (width, (width as f32 / ratio) as u32),
            (None, Some(height)) => ((height as f32 * ratio) as u32, height),
            (Some(width), Some(height)) => (width, height),
        };
    }

    pub fn block_width(&self) -> u32 {
        self.blocks.width()
    }

    pub fn block_height(&self) -> u32 {
        self.blocks.height()
    }
}
