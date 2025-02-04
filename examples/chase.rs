use bevy::{color::palettes::css, prelude::*};
use bevy_behave::prelude::*;

fn main() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins);
    app.add_plugins(BehavePlugin::default());
    app.add_systems(Startup, init);
    app.add_systems(FixedUpdate, (wait_system, move_system));
    app.add_systems(Update, (move_player, render));
    app.add_observer(on_randomize_colour);
    app.run();
}

const COLOURS: [Srgba; 5] = [
    css::CADET_BLUE,
    css::MAGENTA,
    css::GREEN,
    css::YELLOW,
    css::ORANGE,
];

#[derive(Component)]
struct Appearance {
    colour: Color,
    idx: usize,
}

impl Appearance {
    fn new() -> Self {
        Self {
            colour: COLOURS[0].into(),
            idx: 0,
        }
    }
    fn next(&mut self) {
        self.idx = (self.idx + 1) % COLOURS.len();
        self.colour = COLOURS[self.idx].into();
    }
}

#[derive(Component)]
#[require(Transform)]
#[require(Appearance(||Appearance{colour: css::GREEN.into(), idx: 0}))]
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

fn init(mut commands: Commands) {
    commands.spawn(Camera2d);

    let player = commands.spawn((Player,)).id();

    let enemy1 = commands
        .spawn((Enemy, Transform::from_xyz(500., 0., 0.)))
        .id();

    // smaller vision radius on the second enemy
    let enemy2 = commands
        .spawn((
            Enemy,
            Transform::from_xyz(-500., 0., 0.),
            VisionRadius(ENEMY_VISION_RADIUS / 2.0),
        ))
        .id();

    // let t = Behave::trigger_req(RandomizeColour);
    // let tree = tree! {
    //     t
    // };
    // let bt = BehaveTree::new(tree);
    // info!("bt created");

    let tree = tree! {
        Behave::Forever => {
            Behave::Sequence => {
                Behave::dynamic_spawn((
                    Name::new("Wait until player is near"),
                    WaitUntilPlayerIsNear{player}
                )),
                Behave::Sequence => {
                    Behave::dynamic_spawn((
                        Name::new("Move towards player while in range"),
                        MoveTowardsPlayer{player, speed: ENEMY_SPEED}
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

    // default is to assume the Parent entity is the target the tree is controlling
    commands.spawn(BehaveTree::new(tree)).set_parent(enemy1);

    // commands
    //     .spawn(BehaveTree::new(tree.clone()))
    //     .set_parent(enemy2);
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
    info!("Randomizing color: {ctx:?}");
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

// move the player using arrow keys
fn move_player(
    mut players: Query<&mut Transform, With<Player>>,
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
) {
    players.single_mut().translation += Vec3::new(
        (keys.pressed(KeyCode::ArrowRight) as i32 - keys.pressed(KeyCode::ArrowLeft) as i32) as f32,
        (keys.pressed(KeyCode::ArrowUp) as i32 - keys.pressed(KeyCode::ArrowDown) as i32) as f32,
        0.,
    )
    .normalize_or_zero()
        * PLAYER_SPEED
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
            gizmos
                .circle(
                    trans.translation,
                    vision_radius.0,
                    css::WHITE.with_alpha(0.1),
                )
                .resolution(128);
        }
        if is_enemy {
            gizmos.circle(trans.translation, 10., appearance.colour);
        }
        if is_player {
            gizmos.circle(trans.translation, 10., appearance.colour);
        }
    }
}
