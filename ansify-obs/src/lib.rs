use obs_wrapper::{ obs_register_module, obs_string };
use obs_wrapper::prelude::*;
use obs_wrapper::source::*;
use obs_wrapper::properties::*;
use obs_wrapper::graphics::*;
use ansify::*;
use std::path::PathBuf;
use image::DynamicImage;
use std::slice::from_raw_parts;
use image::ImageBuffer;
use image::GenericImageView;
use image::RgbaImage;
use image::Rgba;
use obs_wrapper::media::VideoDataSourceContext;
use obs_wrapper::media::video::*;

struct ANSIfyFilter {
    source: SourceContext,
    effect: GraphicsEffect,
    image: GraphicsEffectTextureParam,
    sampler: GraphicsSamplerState,
    width: u32,
    ansifier: Option<ANSIfier>,
}

struct ANSIfyModule {
    context: ModuleContext,
}

impl Sourceable for ANSIfyFilter {
    fn get_id() -> ObsString {
        obs_string!("ansify_filter")
    }

    fn get_type() -> SourceType {
        SourceType::FILTER
    }

    fn create(create: &mut CreatableSourceContext<Self>, mut source: SourceContext) -> Self {
        let mut effect = GraphicsEffect::from_effect_string(
            obs_string!(include_str!("./ansify.effect")),
            obs_string!("ansify.effect"),
        )
        .expect("Could not load ansify filter effect!");
        let image = effect.get_effect_param_by_name(obs_string!("image")).expect("AAAA");

        let sampler = GraphicsSamplerState::from(GraphicsSamplerInfo::default());

        let settings = &mut create.settings;
        
        let width = settings.get(obs_string!("width")).unwrap_or(80);
        let palette_path = settings.get(obs_string!("palette_path")).unwrap_or(obs_string!("blocks.yaml"));
        let blocks_path = settings.get(obs_string!("blocks_path")).unwrap_or(obs_string!("palette.yaml"));

        let ansifier = if let (Ok(palette), Ok(blocks)) = (Palette::from(PathBuf::from(palette_path.as_str())), Blocks::from(PathBuf::from(blocks_path.as_str()))) {
            Some(ANSIfier::new(palette, blocks))
        } else {
            None
        };

        source.update_source_settings(settings);

        Self {
            source,
            effect,
            image,
            sampler,
            width,
            ansifier,
        }
    }
}

impl GetNameSource for ANSIfyFilter {
    fn get_name() -> ObsString {
        obs_string!("ANSIfy Filter")
    }
}

impl GetPropertiesSource for ANSIfyFilter {
    fn get_properties(&mut self) -> Properties {
        let mut properties = Properties::new();
        properties
            .add(
                obs_string!("width"),
                obs_string!("Number of characters wide"),
                NumberProp::new_int().with_range(1u32..=256),
            )
            .add(
                obs_string!("palette_path"),
                obs_string!("Path to palette"),
                PathProp::new(PathType::File)
                    .with_filter(obs_string!("YAML (*.yaml *.yml)")),
            )
            .add(
                obs_string!("blocks_path"),
                obs_string!("Path to blocks"),
                PathProp::new(PathType::File).with_filter(obs_string!("YAML (*.yaml *.yml)")),
            );
        
        properties
    }
}

impl GetDefaultsSource for ANSIfyFilter {
    fn get_defaults(setings: &mut DataObj<'_>) {
        setings.set_default::<u32>(obs_string!("width"), 80u32);
    }
}

impl UpdateSource for ANSIfyFilter {
    fn update(&mut self, settings: &mut DataObj, _context: &mut GlobalContext) {
        if let Some(width) = settings.get::<u32>(obs_string!("width")) {
            self.width = width;
        }

        if let (Some(palette_path), Some(blocks_path)) = (settings.get::<ObsString>(obs_string!("palette_path")), settings.get::<ObsString>(obs_string!("blocks_path"))) {
            self.ansifier = if let (Ok(palette), Ok(blocks)) = (Palette::from(PathBuf::from(palette_path.as_str())), Blocks::from(PathBuf::from(blocks_path.as_str()))) {
                Some(ANSIfier::new(palette, blocks))
            } else {
                None
            };
        }
    }
}

impl VideoRenderSource for ANSIfyFilter {
    fn video_render(&mut self, _context: &mut GlobalContext, render: &mut VideoRenderContext) {
        if let Some(ansifier) = &self.ansifier {
            // let video_width = video.width();
            // let video_height = video.height();
            // let original_image = match video.format() {
            //     VideoFormat::RGBA => {
            //         let data_length = video_width * video_height * 4;
            //         let data = Vec::from(unsafe { from_raw_parts(video.data_buffer(0), data_length as usize) });
            //         DynamicImage::ImageRgba8(ImageBuffer::from_raw(video_width, video_height, data).expect("Container too small"))
            //     }
            //     _ => panic!("Bad format")
            // };

            // let width = self.width;

            // let new_dimensions = ansifier
            // .calculate_new_dimensions(original_image.dimensions(), (Some(width), None));
            // let img = original_image
            // .resize_exact(
            //     new_dimensions.0,
            //     new_dimensions.1,
            //     image::imageops::Lanczos3,
            // )
            // .into_rgb8();
            
            // let (out, _text) = ansifier.process(&img);

            // self.img = Some(DynamicImage::ImageRgb8(out).to_rgba8());
            
            let img = ImageBuffer::from_fn(1024, 1024, |_x, _y| {
                Rgba([255, 0, 255, 255])
            });

            let source = &mut self.source;
            let effect = &mut self.effect;
            let image = &mut self.image;
            let sampler = &mut self.sampler;

            let image_cx = img.width();
            let image_cy = img.height();

            let mut texture = GraphicsTexture::new(image_cx, image_cy, GraphicsColorFormat::RGBA);
            texture.set_image(img.into_raw().as_slice(), image_cx * 4, false);
            image.set_texture(&mut texture);

            source.process_filter_tech(
                render,
                effect,
                (image_cx, image_cy),
                GraphicsColorFormat::RGBA,
                GraphicsAllowDirectRendering::NoDirectRendering,
                obs_string!("Draw"),
                |context, _effect| {
                },
            );

            source.effect_loop(
                render,
                effect,
                obs_string!("Draw"),
                |context, _effect| {
                    texture.draw(0, 0, 0, 0, false);
                },
            );
        }
    }
}

impl Module for ANSIfyModule {
    fn new(context: ModuleContext) -> Self {
        Self { context }
    }
    
    fn get_ctx(&self) -> &ModuleContext {
        &self.context
    }

    fn load(&mut self, load_context: &mut LoadContext) -> bool {
        let source = load_context
            .create_source_builder::<ANSIfyFilter>()
            .enable_get_name()
            .enable_get_properties()
            .enable_get_defaults()
            .enable_update()
            .enable_video_render()
            .build();

        load_context.register_source(source);

        true
    }

    fn description() -> ObsString {
        obs_string!("A filter that ANSIfys a source.")
    }
    fn name() -> ObsString {
        obs_string!("ANSIfy")
    }
    fn author() -> ObsString {
        obs_string!("widberg")
    }
}

obs_register_module!(ANSIfyModule);
