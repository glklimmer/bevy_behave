use bevy::{color::palettes::css, prelude::*};
use bevy_behave::prelude::*;
use bevy_pancam::*;
use bevy_screen_diagnostics::{
    ScreenDiagnosticsPlugin, ScreenEntityDiagnosticsPlugin, ScreenFrameDiagnosticsPlugin,
};

fn main() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "bevy_behave chase example".to_string(),
            ..default()
        }),
        ..default()
    }));
    app.add_plugins(ScreenDiagnosticsPlugin::default());
    app.add_plugins(ScreenFrameDiagnosticsPlugin);
    app.add_plugins(ScreenEntityDiagnosticsPlugin);
    app.add_plugins(PanCamPlugin);
    app.add_plugins(BehavePlugin::default());
    app.add_systems(Startup, init);
    app.add_plugins(chase_plugin);
    app.add_plugins(add_help_text);
    app.run();
}

fn chase_plugin(app: &mut App) {
    app.add_systems(Update, (player_movement_system, handle_shortcuts, render));
    app.add_systems(FixedUpdate, (wait_system, move_system));
    app.add_observer(on_randomize_colour);
    app.add_observer(onadd_move_towards_player);
    app.add_observer(onremove_move_towards_player);
    app.add_observer(on_despawn_enemies);
    app.add_observer(on_spawn_enemies);
}

#[derive(Component)]
#[require(Transform)]
#[require(Appearance(||Appearance{colour: (css::GREEN * 5.0).into(), idx: 0, show_vision: false}))]
struct Player;

#[derive(Component)]
#[require(Transform)]
#[require(VisionRadius(||VisionRadius(ENEMY_VISION_RADIUS)))]
#[require(Appearance(Appearance::new))]
struct Enemy;

#[derive(Component)]
struct VisionRadius(f32);

const PLAYER_SPEED: f32 = 200.;
const ENEMY_SPEED: f32 = 100.;
const ENEMY_VISION_RADIUS: f32 = 300.0;
const HELP_MSG: &str = "Arrows = move\nShift+Arrows = move fast\nMouse/Scroll = pan and zoom\nHold S = More Enemies\nHold D = Fewer Enemies";

fn init(mut commands: Commands) {
    commands.spawn((
        Camera2d,
        // enable HDR and bloom so we can make out player pop a bit,
        // which makes it easier to see if you are swarmed by 10,000 enemies at once.
        Camera {
            hdr: true,
            ..default()
        },
        bevy::core_pipeline::tonemapping::Tonemapping::TonyMcMapface,
        bevy::core_pipeline::bloom::Bloom::default(),
        PanCam {
            move_keys: DirectionKeys::NONE,
            ..default()
        },
    ));

    // spawn player
    commands.spawn(Player);

    // spawn enemies
    commands.trigger(SpawnEnemies(1000));
}

#[derive(Event)]
struct SpawnEnemies(usize);

#[derive(Event)]
struct DespawnEnemies(usize);

fn on_despawn_enemies(
    trigger: Trigger<DespawnEnemies>,
    q: Query<Entity, With<Enemy>>,
    mut commands: Commands,
) {
    let num = trigger.event().0;
    let mut i = 0;
    for e in q.iter() {
        commands.entity(e).despawn_recursive();
        i += 1;
        if i >= num {
            break;
        }
    }
}

fn on_spawn_enemies(
    trigger: Trigger<SpawnEnemies>,
    player: Single<Entity, With<Player>>,
    mut commands: Commands,
) {
    // we'll apply this behaviour tree to all enemies we are about to spawn
    // in theory it should be possible to load this structure from a file using reflection.
    let tree = tree! {
        Behave::Forever => {
            Behave::Sequence => {
                Behave::dynamic_spawn((
                    Name::new("Wait until player is near"),
                    WaitUntilPlayerIsNear{player: *player}
                )),
                Behave::Sequence => {
                    Behave::dynamic_spawn((
                        Name::new("Move towards player while in range"),
                        MoveTowardsPlayer{player: *player, speed: ENEMY_SPEED}
                    )),
                    // MoveTowardsPlayer suceeds if we catch them, in which randomize our colour.
                    // This uses a trigger to take an action without spawning an entity.
                    Behave::trigger_req(RandomizeColour),
                    // then have a nap (pause execution of the tree)
                    // NB: this only runs if the trigger_req was successful, since it's in a Sequence.
                    Behave::Wait(5.0),
                }
            }
        }
    };

    let num = trigger.event().0;
    for _ in 0..num {
        // give enemy a random starting position and vision radius
        let x = rand::random::<f32>() * 10000.0 - 5000.0;
        let y = rand::random::<f32>() * 10000.0 - 5000.0;
        let vision_radius =
            rand::random::<f32>() * (ENEMY_VISION_RADIUS / 2.0) + (ENEMY_VISION_RADIUS / 2.0);

        let enemy = commands
            .spawn((
                Enemy,
                VisionRadius(vision_radius),
                Transform::from_xyz(x, y, 0.),
            ))
            .id();

        // default is to assume the Parent entity is the target the tree is controlling,
        // so we add the tree as a child. This way recursivly despawning enemies will remove
        // the tree also.
        commands
            .spawn(BehaveTree::new(tree.clone()))
            .set_parent(enemy);
    }
}

#[derive(Component, Clone)]
struct WaitUntilPlayerIsNear {
    player: Entity,
}

#[derive(Component, Clone)]
struct MoveTowardsPlayer {
    player: Entity,
    speed: f32,
}

// We use RandomizeColour as a trigger in our tree.
// It's not an event or a component, it gets wrapped in a BehaveTrigger struct, which is an Event.
#[derive(Clone)]
struct RandomizeColour;

fn on_randomize_colour(
    trigger: Trigger<BehaveTrigger<RandomizeColour>>,
    mut q: Query<&mut Appearance, With<Enemy>>,
    mut commands: Commands,
) {
    let ev = trigger.event();
    // there wasn't any useful info in our trigger struct, but it's here:
    let _randomise_color: &RandomizeColour = ev.inner();
    let ctx: &BehaveCtx = ev.ctx();
    // info!("Randomizing color: {ctx:?}");
    let mut appearance = q.get_mut(ctx.target_entity()).unwrap();
    appearance.next();
    // report success
    commands.trigger(ctx.success());
}

// Wait until the player is within vision range of an enemy
fn wait_system(
    q: Query<(&WaitUntilPlayerIsNear, &BehaveCtx)>,
    q_enemy_transforms: Query<(&Transform, &VisionRadius), (With<Enemy>, Without<Player>)>,
    q_player_transforms: Query<&Transform, (With<Player>, Without<Enemy>)>,
    mut commands: Commands,
) {
    for (wait, ctx) in &q {
        let player_transform = q_player_transforms.get(wait.player).unwrap();
        let (enemy_transform, vision_radius) = q_enemy_transforms.get(ctx.target_entity()).unwrap();
        let distance_to_player = enemy_transform
            .translation
            .xy()
            .distance(player_transform.translation.xy());
        if distance_to_player < vision_radius.0 {
            commands.trigger(ctx.success());
        }
    }
}

// When the BehaveTree adds or removes our MoveTowardsPlayer component, we'll modify the appearance
// so we only render their vision radius when they are chasing the player.
// Note: we aren't replying with a success or failure in OnAdd (although we could), because this
// component has a `move_system` that runs in FixedUpdate, which does the responding.
fn onadd_move_towards_player(
    trigger: Trigger<OnAdd, MoveTowardsPlayer>,
    q: Query<&BehaveCtx, With<MoveTowardsPlayer>>,
    mut q_target: Query<&mut Appearance, With<Enemy>>,
) {
    let ctx = q.get(trigger.entity()).unwrap();
    let mut appearance = q_target.get_mut(ctx.target_entity()).unwrap();
    appearance.show_vision = true;
}

fn onremove_move_towards_player(
    trigger: Trigger<OnRemove, MoveTowardsPlayer>,
    q: Query<&BehaveCtx, With<MoveTowardsPlayer>>,
    mut q_target: Query<&mut Appearance, With<Enemy>>,
) {
    let ctx = q.get(trigger.entity()).unwrap();
    let mut appearance = q_target.get_mut(ctx.target_entity()).unwrap();
    appearance.show_vision = false;
}

// move the enemy towards the player, as long as they are still in vision range.
// if we catch them, return Success.
// if they move out of range, return Failure.
fn move_system(
    q: Query<(&MoveTowardsPlayer, &BehaveCtx)>,
    mut q_own_transforms: Query<(&mut Transform, &VisionRadius), (With<Enemy>, Without<Player>)>,
    q_player_transforms: Query<&Transform, (With<Player>, Without<Enemy>)>,
    mut commands: Commands,
    time: Res<Time>,
) {
    for (move_towards, ctx) in &q {
        let player_transform = q_player_transforms.get(move_towards.player).unwrap();
        let (mut own_transform, vision_radius) =
            q_own_transforms.get_mut(ctx.target_entity()).unwrap();
        let direction_to_player =
            (player_transform.translation.xy() - own_transform.translation.xy()).normalize();
        let movement_amount = direction_to_player * move_towards.speed * time.delta_secs();
        own_transform.translation += movement_amount.extend(0.0);
        let distance_to_player = own_transform
            .translation
            .distance(player_transform.translation);
        if distance_to_player < 10.0 {
            commands.trigger(ctx.success());
        } else if distance_to_player > vision_radius.0 {
            commands.trigger(ctx.failure());
        }
    }
}

// ---- Not very bevy_behave specific code follows, just making the example work..

fn handle_shortcuts(mut commands: Commands, keys: Res<ButtonInput<KeyCode>>) {
    if keys.pressed(KeyCode::KeyS) {
        commands.trigger(SpawnEnemies(100));
    }
    if keys.pressed(KeyCode::KeyD) {
        commands.trigger(DespawnEnemies(100));
    }
}
// move the player using arrow keys
fn player_movement_system(
    mut players: Query<&mut Transform, With<Player>>,
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
) {
    let speed = if keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight) {
        PLAYER_SPEED * 6.0
    } else {
        PLAYER_SPEED
    };
    players.single_mut().translation += Vec3::new(
        (keys.pressed(KeyCode::ArrowRight) as i32 - keys.pressed(KeyCode::ArrowLeft) as i32) as f32,
        (keys.pressed(KeyCode::ArrowUp) as i32 - keys.pressed(KeyCode::ArrowDown) as i32) as f32,
        0.,
    )
    .normalize_or_zero()
        * speed
        * time.delta_secs();
}

// render with gizmos
fn render(
    mut gizmos: Gizmos,
    q: Query<(
        (&Transform, &Appearance, Option<&VisionRadius>),
        Has<Enemy>,
        Has<Player>,
    )>,
) {
    for ((trans, appearance, vision_radius), is_enemy, is_player) in &q {
        if let Some(vision_radius) = vision_radius {
            if appearance.show_vision {
                gizmos
                    .circle(
                        trans.translation,
                        vision_radius.0,
                        css::WHITE.with_alpha(0.1),
                    )
                    .resolution(64);
            }
        }
        if is_enemy {
            gizmos.rect_2d(trans.translation.xy(), Vec2::new(7., 7.), appearance.colour);
        }
        if is_player {
            gizmos.circle_2d(trans.translation.xy(), 10., appearance.colour);
        }
    }
}

const COLOURS: [Srgba; 6] = [
    css::RED,
    css::MAGENTA,
    css::HOT_PINK,
    css::YELLOW,
    css::CADET_BLUE,
    css::ORANGE,
];

#[derive(Component)]
struct Appearance {
    colour: Color,
    idx: usize,
    show_vision: bool,
}

impl Appearance {
    fn new() -> Self {
        Self {
            colour: COLOURS[0].into(),
            idx: 0,
            show_vision: false,
        }
    }
    fn next(&mut self) {
        self.idx = (self.idx + 1) % COLOURS.len();
        self.colour = COLOURS[self.idx].into();
    }
}

fn add_help_text(app: &mut App) {
    app.world_mut()
        .spawn(Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            padding: UiRect::all(Val::Px(10.0)),
            align_items: AlignItems::FlexStart,
            justify_content: JustifyContent::FlexStart,
            flex_direction: FlexDirection::Row,
            ..default()
        })
        .with_children(|parent| {
            parent.spawn((
                Text(HELP_MSG.to_string()),
                TextColor(Color::srgb(0.9, 0.9, 0.9).with_alpha(0.4)),
                TextFont::from_font_size(18.0),
                Node {
                    padding: UiRect::all(Val::Px(10.0)),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
            ));
        });
}
