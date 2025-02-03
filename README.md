# bevy_behave

A behaviour tree plugin for bevy with dynamic spawning.

```rust
let npc_entity = some_character_to_control();

// the tree definition
let t = tree! {
    Behave::Fallback => {
        Behave::dynamic_spawn((
            SlowAction::failing("Single Slowcoach", 2.0),
            Name::new("Single Slowcoach failing")
        )),
        Behave::AlwaysFail,
        Behave::Invert => {
            Behave::dynamic_spawn((
                SlowAction::succeeding("Single Slowcoach inside invert", 1.0),
                Name::new("Single Slowcoach inside invert")
            )),
        },
        Behave::dynamic_spawn((
            SlowAction::succeeding("Single Slowcoach", 1.0),
            Name::new("Single Slowcoach")
        )),
        Behave::AlwaysSucceed,
    }
};
// the component
let bt = BehaveTree::new(t);
// spawn entity with behavetree. Make it a child of the npc entity for convenience.
// default behaviour assumes the Parent of the tree entity is Target Entity you're controlling.
let bt_ent = commands.spawn((Name::new("Behave tree for NPC"), bt)).set_parent(npc_entity);
```

### Control Flow Nodes

Currently supported control flow nodes:

| Node          | Description                                                                |
| ------------- | -------------------------------------------------------------------------- |
| Sequence      | Runs children in sequence, failing if any fails, succeeding if all succeed |
| Fallback      | Runs children in sequence until one succeeds. If all fail, this fails      |
| Invert        | Inverts success/failure of child. Must only have one child                 |
| AlwaysSucceed | Always succeeds                                                            |
| AlwaysFail    | Always fails                                                               |

### Task Nodes

| Node         | Description                                                                                                                                                                                                     |
| ------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Wait         | Waits this many seconds before Succeeding<br>Timer is ticked inside the tre, no entities are spawned.                                                                                                           |
| DynamicSpawn | Spawns an entity when this node in the tree is reached, and waits for it to trigger a status report.<br>Once the entity triggers a status report, it is immediately despawned.                                  |
| Entity       | (TODO) When this node on the tree is reached, a `BehaveCtx` is inserted.<br>The tree then waits for this entity to trigger a status report.<br>On completion, `BehaveCtx` is removed, but nothing is despawned. |


## Conditional Impl Notes

Could do `If` and `While` control flow by taking an `IntoSystem` with a defined In and Out type,
something like this:
```rust

pub type BoxedConditionSystem = Box<dyn System<In = In<BehaveCtx>, Out = bool>>;

#[derive(Debug)]
pub enum Behave {
    // ...
    /// If, then
    Conditional(BoxedConditionSystem),
}

impl Behave {
    pub fn conditional<Marker>(system: impl IntoSystem<In<BehaveCtx>, bool, Marker>) -> Behave {
        Behave::Conditional(Box::new(IntoSystem::into_system(system)))
    }
}
```

Then you could defined a cond system like, which is quite convenient:

```rust
fn check_distance(In(ctx): In<BehaveCtx>, q: Query<&Position, With<Player>>) -> bool {
    let Ok(player_pos) = q.get(ctx.target_entity).unwrap();
    player_pos.x < 100.0
}
```


However I don't think the resulting data struct would be cloneable, nor could you really read
it from an asset file for manipulation (how do you serialize a system like that?).
I would also need mutable World in the "tick trees" system, to run the conditionals, which removes
any chance of concurrency (i think).

Instead, can we require something that derives bevy Event, trigger it, and expect the observer
that the user wired up to provide an output, then pipe it into something to save the result?

Could i say:

```rust
let t = tree! {
    Conditional(DistanceCheck) => {
        // do something
    }
}

#[derive(Event)]
struct DistanceCheck(f32)

fn on_distance_check(trigger: Trigger<Conditional<DistanceCheck>>, q: Query<&Position, With<Player>>) {
    let distance_check = trigger.event().value();
    let mut ctx = trigger.event().ctx();

    let Ok(player_pos) = q.get(ctx.target_entity).unwrap();
    // can we use observer propagation to grab this result?
    trigger.event().set_result(player_pos.x < distance_check.0);
}
```

Then the condition event struct could be Clone, and Reflect – so loaded from an asset file.

