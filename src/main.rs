use ansi_term::Colour::Fixed;
use clap::{Parser, Subcommand};
use image::gif::{GifDecoder, GifEncoder, Repeat};
use image::io::Reader as ImageReader;
use image::AnimationDecoder;
use image::Frame;
use image::{DynamicImage, GenericImageView, RgbImage};
use kd_tree::KdMap;
use log::info;
use nokhwa::Camera;
use serde::{Deserialize, Serialize};
use show_image::create_window;
use show_image::WindowOptions;
use std::collections::BTreeMap;
use std::fs::File;
use std::path::PathBuf;
use std::vec::Vec;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    #[arg(short, long, value_name = "PALETTE_PATH")]
    palette: PathBuf,

    #[arg(short, long, value_name = "BLOCKS_PATH")]
    blocks: PathBuf,

    #[arg(short, long, value_name = "WIDTH")]
    width: Option<u32>,

    #[arg(short = 'H', long, value_name = "HEIGHT")]
    height: Option<u32>,
}

#[derive(Subcommand)]
enum Commands {
    Image {
        #[arg(short, long, value_name = "INPUT_PATH")]
        input: PathBuf,

        #[arg(short, long, value_name = "OUTPUT_PATH")]
        output: Option<PathBuf>,

        #[arg(short, long)]
        text: bool,

        #[arg(short, long)]
        show: bool,
    },
    Gif {
        #[arg(short, long, value_name = "INPUT_PATH")]
        input: PathBuf,

        #[arg(short, long, value_name = "OUTPUT_PATH")]
        output: PathBuf,
    },
    Webcam {
        #[arg(short, long)]
        index: usize,
    },
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

struct ANSIfier {
    palette: Palette,
    blocks: Blocks,
    kdtree: KdMap<[f32; 3], Texel>,
}

impl ANSIfier {
    fn new(palette: Palette, blocks: Blocks) -> ANSIfier {
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
            kdtree: KdMap::par_build_by_ordered_float(texels),
        };
    }

    fn process(&self, img: &RgbImage) -> (RgbImage, String) {
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
            for i in 0..self.blocks.width {
                for j in 0..self.blocks.height {
                    out.put_pixel(
                        x * self.blocks.width + i,
                        y * self.blocks.height + j,
                        if self.blocks.blocks[&texel.block][j as usize][i as usize] {
                            let foreground_color =
                                self.palette.colors[texel.foreground_color as usize];
                            image::Rgb {
                                0: foreground_color,
                            }
                        } else {
                            let background_color =
                                self.palette.colors[texel.background_color as usize];
                            image::Rgb {
                                0: background_color,
                            }
                        },
                    );
                }
            }
        }

        return (out, text);
    }
}

fn calculate_new_dimensions(
    original_dimensions: (u32, u32),
    new_dimensions: (Option<u32>, Option<u32>),
    block_dimensions: (u32, u32),
) -> (u32, u32) {
    info!("Calculating dimension and resizing");

    let ratio = (original_dimensions.0 as f32 / block_dimensions.0 as f32)
        / (original_dimensions.1 as f32 / block_dimensions.1 as f32);

    return match new_dimensions {
        (None, None) => original_dimensions,
        (Some(width), None) => (width, (width as f32 / ratio) as u32),
        (None, Some(height)) => ((height as f32 * ratio) as u32, height),
        (Some(width), Some(height)) => (width, height),
    };
}

impl Palette {
    fn from(path: PathBuf) -> Result<Palette, Box<dyn std::error::Error>> {
        info!("Opening and parsing palette");

        let file = File::open(path)?;
        return Ok(serde_yaml::from_reader(&file)?);
    }
}

impl Blocks {
    fn from(path: PathBuf) -> Result<Blocks, Box<dyn std::error::Error>> {
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
}

#[show_image::main]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    env_logger::init();

    let palette = Palette::from(cli.palette)?;
    let blocks = Blocks::from(cli.blocks)?;
    let block_dimensions = (blocks.width, blocks.height);
    let ansifier = ANSIfier::new(palette, blocks);

    match &cli.command {
        Commands::Image {
            input,
            output,
            text,
            show,
        } => {
            info!("Opening original image");
            let original_image = ImageReader::open(input)?.decode()?;

            info!("Calculating dimension and resizing");
            let new_dimensions = calculate_new_dimensions(
                original_image.dimensions(),
                (cli.width, cli.height),
                block_dimensions,
            );
            let img = original_image
                .resize_exact(
                    new_dimensions.0,
                    new_dimensions.1,
                    image::imageops::Lanczos3,
                )
                .into_rgb8();

            let (out, out_text) = ansifier.process(&img);

            if *text {
                print!("{}", out_text);
            }

            if let Some(output_path) = output {
                info!("Writing output");

                out.save(output_path)?;
            }

            if *show {
                info!("Showing image");

                let window = create_window(
                    "img2ansi",
                    WindowOptions::new().set_size([
                        new_dimensions.0 * block_dimensions.0,
                        new_dimensions.1 * block_dimensions.1,
                    ]),
                )?;
                window.set_image("image", out)?;
                window.wait_until_destroyed()?;
            }
        }
        Commands::Gif { input, output } => {
            info!("Opening original image");
            let file_in = File::open(input)?;
            let decoder = GifDecoder::new(file_in)?;

            let file_out = File::create(output)?;
            let mut encoder = GifEncoder::new(file_out);
            encoder.set_repeat(Repeat::Infinite)?;

            for frame in decoder.into_frames() {
                let frame = frame?;
                info!("Calculating dimension and resizing");
                let left = frame.left();
                let top = frame.top();
                let delay = frame.delay();
                let original_image = DynamicImage::ImageRgba8(frame.into_buffer());

                let new_dimensions = calculate_new_dimensions(
                    original_image.dimensions(),
                    (cli.width, cli.height),
                    block_dimensions,
                );
                let img = original_image
                    .resize_exact(
                        new_dimensions.0,
                        new_dimensions.1,
                        image::imageops::Lanczos3,
                    )
                    .into_rgb8();

                let (out, _) = ansifier.process(&img);

                let left =
                    (left as f32 / original_image.width() as f32 * new_dimensions.0 as f32) as u32;
                let top =
                    (top as f32 / original_image.height() as f32 * new_dimensions.1 as f32) as u32;

                encoder.encode_frame(Frame::from_parts(
                    DynamicImage::ImageRgb8(out).to_rgba8(),
                    left,
                    top,
                    delay,
                ))?;
            }
        }
        Commands::Webcam { index } => {
            info!("Creating webcam");
            let mut camera = Camera::new(*index, None)?;
            camera.open_stream()?;

            info!("Getting webcame image");
            let original_image = camera.frame()?;

            info!("Calculating dimension and resizing");

            let new_dimensions = calculate_new_dimensions(
                original_image.dimensions(),
                (cli.width, cli.height),
                block_dimensions,
            );

            info!("Creating image window");

            let window = create_window(
                "img2ansi",
                WindowOptions::new().set_size([
                    new_dimensions.0 * block_dimensions.0,
                    new_dimensions.1 * block_dimensions.1,
                ]),
            )?;

            loop {
                let original_image = camera.frame()?;

                let img = DynamicImage::ImageRgb8(original_image)
                    .resize_exact(
                        new_dimensions.0,
                        new_dimensions.1,
                        image::imageops::Lanczos3,
                    )
                    .into_rgb8();

                let (out, _) = (&ansifier).process(&img);

                info!("Showing image");

                if window.set_image("image", out).is_err() {
                    info!("Closing window");

                    break;
                }
            }
        }
    }

    info!("Done");

    return Ok(());
}
