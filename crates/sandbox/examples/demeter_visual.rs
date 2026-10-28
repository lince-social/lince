use bevy::{
    camera::{ImageRenderTarget, RenderTarget},
    input::{
        ButtonState,
        keyboard::{Key, KeyboardInput},
    },
    input_focus::{FocusCause, InputFocus},
    math::DVec2,
    prelude::*,
    render::{
        render_resource::{PollType, TextureFormat},
        renderer::RenderDevice,
        view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    },
};
use lince_sandbox::{container::BoxRoot, demeter_castle::DemeterCastle};
use nucleus::{demeter::*, optimization::Options};
use serde_json::json;
use std::{
    path::PathBuf,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

#[derive(Resource)]
struct Output(RenderTarget);

#[derive(Resource, Default)]
struct Captured(bool);

fn output(world: &mut World) {
    let target = RenderTarget::Image(ImageRenderTarget {
        handle: world
            .resource_mut::<Assets<Image>>()
            .add(Image::new_target_texture(
                1280,
                960,
                TextureFormat::Bgra8UnormSrgb,
                None,
            )),
        scale_factor: 1.0,
    });
    let scene = world
        .resource::<lince_sandbox::topology::presentation::SceneCamera>()
        .0;
    let background = world
        .resource::<lince_sandbox::topology::presentation::BackgroundCamera>()
        .0;
    world.entity_mut(scene).insert(target.clone());
    world.entity_mut(background).insert(target.clone());
    world.insert_resource(Output(target));
}

fn step(app: &mut App) {
    app.update();
    app.world()
        .resource::<RenderDevice>()
        .wgpu_device()
        .poll(PollType::Wait {
            submission_index: None,
            timeout: None,
        })
        .unwrap();
    std::thread::sleep(Duration::from_millis(5));
}

fn capture(app: &mut App, path: PathBuf) {
    app.world_mut().resource_mut::<Captured>().0 = false;
    let target = app.world().resource::<Output>().0.clone();
    app.world_mut()
        .spawn(Screenshot(target))
        .observe(save_to_disk(path.clone()))
        .observe(
            |event: On<ScreenshotCaptured>, mut captured: ResMut<Captured>| {
                let bytes = event.image.data.as_ref().unwrap();
                assert!(bytes.chunks_exact(4).any(|pixel| pixel != &bytes[..4]));
                captured.0 = true;
            },
        );
    let started = Instant::now();
    while !app.world().resource::<Captured>().0 {
        step(app);
        assert!(started.elapsed() < Duration::from_secs(30));
    }
    println!("Captured {}", path.display());
}

fn control(world: &mut World, caption: &str) -> Entity {
    world
        .query::<(Entity, &Text)>()
        .iter(world)
        .find_map(|(entity, text)| {
            if text.0 != caption {
                return None;
            }
            let parent = world.get::<ChildOf>(entity)?.parent();
            world
                .get::<bevy::ui_widgets::Button>(parent)
                .map(|_| parent)
        })
        .unwrap()
}

fn activate(world: &mut World, caption: &str) {
    let button = control(world, caption);
    world.trigger(bevy::ui_widgets::Activate { entity: button });
    world.flush();
}

fn fixture() -> DemeterCastle {
    let food = nucleus::new_uid("r");
    let offer = nucleus::new_uid("r");
    let process = nucleus::new_uid("r");
    let mut config = engine::demeter::Config::new("2026-10-25".into());
    config.name = "Example weekly food quantities".into();
    config.horizon_days = 35;
    config.manual.bounds = vec![
        NutrientBound {
            nutrient: "energy_kcal".into(),
            unit: Unit::Kcal,
            bounds: Bounds {
                lower: Some(exact("200").unwrap()),
                upper: None,
            },
            scope: "total".into(),
            source: "Illustrative manual target".into(),
        },
        NutrientBound {
            nutrient: "protein_g".into(),
            unit: Unit::G,
            bounds: Bounds {
                lower: Some(exact("20").unwrap()),
                upper: None,
            },
            scope: "total".into(),
            source: "Illustrative manual target".into(),
        },
    ];
    config.foods.push(FoodChoice {
        food: food.clone(),
        bounds: Vec::new(),
        acquisition: Acquisition::Grow,
        volume_ml_per_g: None,
    });
    config.offers.push(offer.clone());
    let market = Offer {
        id: offer.clone(),
        food: food.clone(),
        currency: "brl".into(),
        price: exact("2").unwrap(),
        package_g: exact("500").unwrap(),
        usable_yield: exact("1").unwrap(),
        available_on: config.start.clone(),
        valid_until: None,
        max_packages: None,
    };
    let input = Input {
        start: config.start.clone(),
        days: config.days,
        currency: config.currency.clone(),
        requirements: config.manual.clone(),
        foods: vec![
            Food::from_nutrition(
                config.foods[0].clone(),
                "Example cooked beans — measured preparation without added salt".into(),
                &json!({"basis_g":100,"nutrients":{"energy_kcal":100,"protein_g":10}}),
            )
            .unwrap(),
        ],
        offers: vec![market.clone()],
        stock: Vec::new(),
        forecasts: Vec::new(),
        volume_ml_per_day: None,
    };
    let result = input
        .solve(Options::default(), &AtomicBool::new(false))
        .unwrap();
    let template:production::Template = serde_json::from_value(json!({"id":process,"food_outputs":[food],"species":"phaseolus-vulgaris","method":"example field crop","first_harvest_days":14,"harvest_interval_days":0,"harvest_count":1,"usable_g_per_harvest":"500","season_months":[1,2,3,4,5,6,7,8,9,10,11,12],"requires_confirmation":true,"assumption":"Visual fixture only; replace with measured local parameters.","inputs":[{"item":"seed","quantity":"1","unit":"each"}],"stages":[{"kind":"care","offset_days":1,"repeat_every_days":7,"title":"Inspect moisture and plant health"}]})).unwrap();
    let choice:production::Choice = serde_json::from_value(json!({"food":food,"template":process,"confirmed":true,"start_on":config.start,"season_months":[1,2,3,4,5,6,7,8,9,10,11,12],"usable_yield":exact("1").unwrap(),"shelf_life_days":7,"cost_per_usable_g":exact("0.001").unwrap(),"currency":"brl","supermarket_fallback":true})).unwrap();
    let projection = production::project(
        &input,
        &result,
        std::slice::from_ref(&choice),
        std::slice::from_ref(&template),
        35,
        false,
    )
    .unwrap();
    config.production.push(choice);
    DemeterCastle {
        config,
        selected_food: Some(food.clone()),
        catalog: json!({"foods":[{"uid":food,"label":"Example cooked beans — measured preparation without added salt"}],"offers":[{"uid":offer,"valid":true,"market":{"offer":market,"vendor":"Local market example","observed_on":"2026-10-25"}}],"templates":[{"uid":process,"template":template,"foods":[food]}],"plans":[],"references":[],"vocabulary":{}}),
        result: Some(result),
        projection: Some(projection),
        changed: false,
        stale: false,
        ..Default::default()
    }
}

fn keyboard_navigation(app: &mut App) {
    let window = app
        .world_mut()
        .query_filtered::<Entity, With<Window>>()
        .single(app.world())
        .unwrap();
    let button = control(app.world_mut(), "Requirements");
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(button, FocusCause::Navigated);
    for state in [ButtonState::Pressed, ButtonState::Released] {
        app.world_mut().write_message(KeyboardInput {
            window,
            key_code: KeyCode::Tab,
            logical_key: Key::Tab,
            state,
            text: None,
            repeat: false,
        });
        step(app);
    }
    let next = app
        .world()
        .resource::<InputFocus>()
        .get()
        .expect("Tab must focus a native control");
    assert_ne!(next, button);
}

fn controls_have_visible_labels(world: &mut World, castle: Entity) {
    let width = world.get::<ComputedNode>(castle).unwrap().size().x;
    for (entity, computed, children) in world
        .query_filtered::<(Entity, &ComputedNode, &Children), With<bevy::ui_widgets::Button>>()
        .iter(world)
    {
        let mut at = entity;
        while let Some(parent) = world.get::<ChildOf>(at) {
            at = parent.parent();
            if at == castle {
                break;
            }
        }
        if at != castle {
            continue;
        }
        for child in children {
            if let Some(text) = world.get::<Text>(*child) {
                let label = world.get::<ComputedNode>(*child).unwrap().size();
                assert!(
                    label.x > 1.0 && label.y > 1.0,
                    "Empty control label: {}",
                    text.0
                );
                assert!(
                    computed.size().x >= 40.0,
                    "Collapsed control label: {} ({:?})",
                    text.0,
                    computed.size()
                );
                assert!(
                    computed.size().x <= width - 18.0,
                    "Control exceeds the Castle width: {}",
                    text.0
                );
            }
        }
    }
}

fn reveal(app: &mut App, caption: &str) {
    let target = control(app.world_mut(), caption);
    let mut viewport = target;
    while app
        .world()
        .get::<lince_sandbox::scroll_sand::ScrollSand>(viewport)
        .is_none()
    {
        viewport = app.world().get::<ChildOf>(viewport).unwrap().parent();
    }
    let shift = app
        .world()
        .get::<UiGlobalTransform>(target)
        .unwrap()
        .translation
        .y
        - app
            .world()
            .get::<UiGlobalTransform>(viewport)
            .unwrap()
            .translation
            .y;
    let mut scroll = app.world_mut().get_mut::<ScrollPosition>(viewport).unwrap();
    scroll.0.y = (scroll.0.y + shift).max(0.0);
    for _ in 0..12 {
        step(app);
    }
}

#[tokio::main]
async fn main() {
    let directory = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/home/user/lince-demeter-work/visual"));
    std::fs::create_dir_all(&directory).unwrap();
    let temporary = tempfile::tempdir_in(&directory).unwrap();
    utils::config::set_lince_data_dir_override(temporary.path().join("lince")).unwrap();
    let mut app = lince_sandbox::app::offscreen_interface_app();
    app.insert_resource(lince_sandbox::workspace::WorkspaceFile::new(
        temporary.path().join("interface.json"),
    ))
    .init_resource::<Captured>()
    .add_systems(
        Startup,
        |mut commands: Commands, mut windows: Query<&mut Window>| {
            windows.single_mut().unwrap().resolution.set(1280.0, 960.0);
            commands.spawn(BoxRoot);
        },
    )
    .add_systems(PostStartup, output);
    app.finish();
    app.cleanup();
    step(&mut app);
    let root = app
        .world_mut()
        .query_filtered::<Entity, With<BoxRoot>>()
        .single(app.world())
        .unwrap();
    let castle =
        lince_sandbox::demeter_castle::spawn(app.world_mut(), root, 1, DVec2::ZERO, fixture());
    for _ in 0..12 {
        step(&mut app);
    }
    keyboard_navigation(&mut app);
    for width in [720.0, 360.0] {
        app.world_mut()
            .get_mut::<lince_sandbox::canvas::CanvasItem>(castle)
            .unwrap()
            .size = Vec2::new(width, 820.0);
        for (caption, name) in [
            ("Requirements", "requirements"),
            ("Foods / prices", "foods"),
            ("Results", "results"),
            ("Acquire / grow", "production"),
        ] {
            activate(app.world_mut(), caption);
            for _ in 0..12 {
                step(&mut app);
            }
            controls_have_visible_labels(app.world_mut(), castle);
            capture(
                &mut app,
                directory.join(format!("demeter-{name}-{width:.0}.png")),
            );
            if name == "foods" {
                reveal(&mut app, "Save price assertion");
                capture(
                    &mut app,
                    directory.join(format!("demeter-prices-{width:.0}.png")),
                );
            }
            if name == "production" {
                reveal(&mut app, "Accept reviewed tasks");
                capture(
                    &mut app,
                    directory.join(format!("demeter-tasks-{width:.0}.png")),
                );
            }
        }
    }
}
