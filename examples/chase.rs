use bevy::{color::palettes::css, prelude::*};
use bevy_behave::prelude::*;

fn main() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins);
    app.add_plugins(BehavePlugin::default());
    app.add_systems(Startup, init);
    app.add_systems(FixedUpdate, (wait_system, move_system));
    app.add_systems(Update, (move_player, render));
    app.run();
}

#[derive(Component)]
#[require(Transform)]
struct Player;

#[derive(Component)]
#[require(Transform)]
struct Enemy;

#[derive(Component)]
struct VisionRadius(f32);

const PLAYER_SPEED: f32 = 200.;
const ENEMY_SPEED: f32 = 100.;
const ENEMY_VISION_RADIUS: f32 = 300.0;

fn init(mut commands: Commands) {
    commands.spawn(Camera2d);

    let player = commands.spawn((Player,)).id();

    let enemy = commands
        .spawn((
            Transform::from_xyz(500., 0., 0.),
            Enemy,
            VisionRadius(ENEMY_VISION_RADIUS),
        ))
        .id();

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
                    // MoveTowardsPlayer suceeds if we catch them, in which case have a nap:
                    Behave::Wait(5.0),
                }
            }
        }
    };

    // default is to assume the Parent entity is the target the tree is controlling
    commands.spawn(BehaveTree::new(tree)).set_parent(enemy);
}

fn render(
    mut gizmos: Gizmos,
    q: Query<((&Transform, Option<&VisionRadius>), Has<Enemy>, Has<Player>)>,
) {
    for ((trans, vision_radius), is_enemy, is_player) in &q {
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
            gizmos.circle(trans.translation, 10., css::RED);
        }
        if is_player {
            gizmos.circle(trans.translation, 10., css::GREEN);
        }
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

fn move_system(
    q: Query<(&MoveTowardsPlayer, &BehaveCtx)>,
    mut q_own_transforms: Query<&mut Transform, (With<Enemy>, Without<Player>)>,
    q_player_transforms: Query<&Transform, (With<Player>, Without<Enemy>)>,
    mut commands: Commands,
    time: Res<Time>,
) {
    for (move_towards, ctx) in &q {
        let player_transform = q_player_transforms.get(move_towards.player).unwrap();
        let mut own_transform = q_own_transforms.get_mut(ctx.target_entity()).unwrap();
        let direction_to_player =
            (player_transform.translation.xy() - own_transform.translation.xy()).normalize();
        let movement_amount = direction_to_player * move_towards.speed * time.delta_secs();
        own_transform.translation += movement_amount.extend(0.0);
        let distance_to_player = own_transform
            .translation
            .distance(player_transform.translation);
        if distance_to_player < 10.0 {
            commands.trigger(ctx.success());
        } else if distance_to_player > ENEMY_VISION_RADIUS {
            commands.trigger(ctx.failure());
        }
    }
}

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
