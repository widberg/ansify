use obs_wrapper::{ obs_register_module, obs_string };
use obs_wrapper::prelude::*;
use obs_wrapper::source::*;
use obs_wrapper::properties::*;
use obs_wrapper::graphics::*;
use obs_wrapper::log::Logger;
use std::path::PathBuf;
use ansify::{ANSIfier, Blocks, Palette};

struct ANSIfyFilter {
    image: GraphicsEffectTextureParam,
    source: SourceContext,
    effect: GraphicsEffect,
    sampler: GraphicsSamplerState,

    ansifier_data: Option<(ANSIfier, GraphicsTexture, GraphicsTexture)>,
    width: u32,
    palette_path_setting: Option::<ObsString>,
    blocks_path_setting: Option::<ObsString>,

    lut: GraphicsEffectTextureParam,
    map: GraphicsEffectTextureParam,
    character_dimensions: GraphicsEffectVec2Param,
    image_dimensions: GraphicsEffectVec2Param,
    image_dimensions_i: GraphicsEffectVec2Param,
    characters: GraphicsEffectVec2Param,
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
        .expect("Could not load crop filter effect!");

        let settings = &mut create.settings;
        
        if let (
            Some(image),
            Some(lut),
            Some(map),
            Some(character_dimensions),
            Some(image_dimensions),
            Some(image_dimensions_i),
            Some(characters),
        ) = (
            effect.get_effect_param_by_name(obs_string!("image")),
            effect.get_effect_param_by_name(obs_string!("lut")),
            effect.get_effect_param_by_name(obs_string!("map")),
            effect.get_effect_param_by_name(obs_string!("character_dimensions")),
            effect.get_effect_param_by_name(obs_string!("image_dimensions")),
            effect.get_effect_param_by_name(obs_string!("image_dimensions_i")),
            effect.get_effect_param_by_name(obs_string!("characters")),
        ) {
            let width = settings.get(obs_string!("width")).unwrap_or(80u32);
            let palette_path_setting: Option::<ObsString> = settings.get(obs_string!("palette_path"));
            let blocks_path_setting: Option::<ObsString> = settings.get(obs_string!("blocks_path"));

            let ansifier_data = if let (Some(palette_path), Some(blocks_path)) = (palette_path_setting.clone(), blocks_path_setting.clone()) {
                if let (Ok(palette), Ok(blocks)) = (
                    Palette::from(PathBuf::from(palette_path.as_str())),
                    Blocks::from(PathBuf::from(blocks_path.as_str()))) {
                    let ansifier = ANSIfier::new(palette, blocks);
            
                    #[cfg(feature = "rayon")]
                    let (lut_image_buffer, map_image_buffer) = ansifier.par_generate_lut_and_map();
                    #[cfg(not(feature = "rayon"))]
                    let (lut_image_buffer, map_image_buffer) = ansifier.generate_lut_and_map();
            
                    let lut_image_buffer_dimensions = lut_image_buffer.dimensions();
                    let mut lut_texture = GraphicsTexture::new(lut_image_buffer_dimensions.0, lut_image_buffer_dimensions.1, GraphicsColorFormat::RGBA);
                    let lut_raw = lut_image_buffer.into_raw();
                    lut_texture.set_image(lut_raw.as_slice(), lut_image_buffer_dimensions.0 * 4, false);
            
                    let map_image_buffer_dimensions = map_image_buffer.dimensions();
                    let mut map_texture = GraphicsTexture::new(map_image_buffer_dimensions.0, map_image_buffer_dimensions.1, GraphicsColorFormat::RGBA);
                    let map_raw = map_image_buffer.into_raw();
                    map_texture.set_image(map_raw.as_slice(), map_image_buffer_dimensions.0 * 4, false);

                    Some((ansifier, lut_texture, map_texture))
                } else {
                    None
                }
            } else {
                None
            };

            let sampler = GraphicsSamplerState::from(GraphicsSamplerInfo::default()
                .with_address_u(GraphicsAddressMode::Clamp)
                .with_address_v(GraphicsAddressMode::Clamp)
                .with_address_w(GraphicsAddressMode::Clamp)
                .with_filter(GraphicsSampleFilter::Point));

            source.update_source_settings(settings);

            return Self {
                image,
                source,
                effect,
                sampler,

                ansifier_data,
                width,
                palette_path_setting,
                blocks_path_setting,

                lut,
                map,
                character_dimensions,
                image_dimensions,
                image_dimensions_i,
                characters,
            };
        }

        panic!("Failed to find correct effect params!");
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
                NumberProp::new_int().with_range(1u32..=1024).with_slider(),
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
                PathProp::new(PathType::File)
                    .with_filter(obs_string!("YAML (*.yaml *.yml)")),
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

        let palette_path_setting: Option::<ObsString> = settings.get(obs_string!("palette_path"));
        let blocks_path_setting: Option::<ObsString> = settings.get(obs_string!("blocks_path"));

        if self.palette_path_setting != palette_path_setting || self.blocks_path_setting != blocks_path_setting {
            self.ansifier_data = if let (Some(palette_path), Some(blocks_path)) = (palette_path_setting, blocks_path_setting) {
                if let (Ok(palette), Ok(blocks)) = (
                    Palette::from(PathBuf::from(palette_path.as_str())),
                    Blocks::from(PathBuf::from(blocks_path.as_str()))) {
                    let ansifier = ANSIfier::new(palette, blocks);
            
                    #[cfg(feature = "rayon")]
                    let (lut_image_buffer, map_image_buffer) = ansifier.par_generate_lut_and_map();
                    #[cfg(not(feature = "rayon"))]
                    let (lut_image_buffer, map_image_buffer) = ansifier.generate_lut_and_map();
            
                    let lut_image_buffer_dimensions = lut_image_buffer.dimensions();
                    let mut lut_texture = GraphicsTexture::new(lut_image_buffer_dimensions.0, lut_image_buffer_dimensions.1, GraphicsColorFormat::RGBA);
                    let lut_raw = lut_image_buffer.into_raw();
                    lut_texture.set_image(lut_raw.as_slice(), lut_image_buffer_dimensions.0 * 4, false);
            
                    let map_image_buffer_dimensions = map_image_buffer.dimensions();
                    let mut map_texture = GraphicsTexture::new(map_image_buffer_dimensions.0, map_image_buffer_dimensions.1, GraphicsColorFormat::RGBA);
                    let map_raw = map_image_buffer.into_raw();
                    map_texture.set_image(map_raw.as_slice(), map_image_buffer_dimensions.0 * 4, false);

                    Some((ansifier, lut_texture, map_texture))
                } else {
                    None
                }
            } else {
                None
            };
        }
    }
}

impl VideoRenderSource for ANSIfyFilter {
    fn video_render(&mut self, _context: &mut GlobalContext, render: &mut VideoRenderContext) {
        let data = self;

        if let Some(ansifier_data) = &mut data.ansifier_data {
            let ansifier = &mut ansifier_data.0;
            let lut_texture = &mut ansifier_data.1;
            let map_texture = &mut ansifier_data.2;
            let width = data.width;

            let image = &mut data.image;
            let effect = &mut data.effect;
            let source = &mut data.source;
            let sampler = &mut data.sampler;

            let lut = &mut data.lut;
            let map = &mut data.map;
            let character_dimensions = &mut data.character_dimensions;
            let image_dimensions = &mut data.image_dimensions;
            let image_dimensions_i = &mut data.image_dimensions_i;
            let characters = &mut data.characters;

            let mut target_cx: u32 = 1;
            let mut target_cy: u32 = 1;

            let cx = source.get_base_width();
            let cy = source.get_base_height();
            let dimensions = ansifier.calculate_new_dimensions((cx, cy), (Some(width), None));

            source.do_with_target(|target| {
                target_cx = target.get_base_width();
                target_cy = target.get_base_height();
            });

            source.process_filter_tech(
                render,
                effect,
                (target_cx, target_cy),
                GraphicsColorFormat::RGBA,
                GraphicsAllowDirectRendering::NoDirectRendering,
                obs_string!("Draw"),
                |context, _effect| {
                    lut.set_texture(context, &lut_texture);
                    map.set_texture(context, &map_texture);
                    character_dimensions.set_vec2(context, &Vec2::new(ansifier.block_width() as f32, ansifier.block_height() as f32));
                    image_dimensions.set_vec2(context, &Vec2::new(cx as _, cy as _));
                    image_dimensions_i.set_vec2(context, &Vec2::new(1. / (cx as f32), 1. / (cy as f32)));
                    characters.set_vec2(context, &Vec2::new(dimensions.0 as f32, dimensions.1 as f32));

                    image.set_next_sampler(context, sampler);
                },
            );
        }
    }
}

impl Module for ANSIfyModule {
    fn new(context: ModuleContext) -> Self {
        let _ = Logger::new().init();

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
