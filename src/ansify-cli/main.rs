use ansify::{ANSIfier, Blocks, Palette};
use clap::{Parser, Subcommand};
use image::gif::{GifDecoder, GifEncoder, Repeat};
use image::io::Reader as ImageReader;
use image::{AnimationDecoder, Delay, DynamicImage, Frame, GenericImageView};
use log::info;
use nokhwa::Camera;
use show_image::create_window;
use show_image::WindowOptions;
use std::fs::File;
use std::path::PathBuf;
use std::time::Instant;

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

        #[arg(short, long, value_name = "OUTPUT_PATH")]
        output: Option<PathBuf>,
    },
}

#[show_image::main]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    env_logger::init();

    let palette = Palette::from(cli.palette)?;
    let blocks = Blocks::from(cli.blocks)?;
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
            let new_dimensions = ansifier
                .calculate_new_dimensions(original_image.dimensions(), (cli.width, cli.height));
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
                        new_dimensions.0 * ansifier.block_width(),
                        new_dimensions.1 * ansifier.block_height(),
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

                let new_dimensions = ansifier
                    .calculate_new_dimensions(original_image.dimensions(), (cli.width, cli.height));
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
        Commands::Webcam { index, output } => {
            info!("Creating webcam");
            let mut camera = Camera::new(*index, None)?;
            camera.open_stream()?;

            info!("Getting webcame image");
            let original_image = camera.frame()?;

            info!("Calculating dimension and resizing");

            let new_dimensions = ansifier
                .calculate_new_dimensions(original_image.dimensions(), (cli.width, cli.height));

            info!("Creating image window");

            let window = create_window(
                "img2ansi",
                WindowOptions::new().set_size([
                    new_dimensions.0 * ansifier.block_width(),
                    new_dimensions.1 * ansifier.block_height(),
                ]),
            )?;

            let mut encoder = if let Some(output_file) = output {
                info!("Gif file");

                let file_out = File::create(output_file)?;
                let mut new_encoder = GifEncoder::new(file_out);
                new_encoder.set_repeat(Repeat::Infinite)?;
                Some(new_encoder)
            } else {
                None
            };

            let mut last_frame = (None, Instant::now());

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

                if let Some(ref mut enc) = encoder {
                    if let (Some(real_last_frame), last_time) = last_frame {
                        enc.encode_frame(Frame::from_parts(
                            real_last_frame,
                            0,
                            0,
                            Delay::from_saturating_duration(last_time.elapsed()),
                        ))?;
                    }

                    last_frame = (
                        Some(DynamicImage::ImageRgb8(out.clone()).to_rgba8()),
                        Instant::now(),
                    );
                }

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
