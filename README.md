# bevy_behave

A behaviour tree plugin for bevy with dynamic spawning.

When an action (leaf node / task node) in the behaviour tree runs, it will spawn an entity with
the components you specified in the tree definition. The tree then waits for this entity to
trigger a status report, at which point the entity will be despawned.



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

| Node          | Description                                                                                 |
| ------------- | ------------------------------------------------------------------------------------------- |
| Sequence      | Runs children in sequence, failing if any fails, succeeding if all succeed                  |
| Fallback      | Runs children in sequence until one succeeds. If all fail, this fails                       |
| Invert        | Inverts success/failure of child. Must only have one child                                  |
| AlwaysSucceed | Always succeeds                                                                             |
| AlwaysFail    | Always fails                                                                                |
| Conditional   | Triggers an event, which the user observes and responds to with a success or failure report |

### Task Nodes

| Node         | Description                                                                                                                                                                    |
| ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Wait         | Waits this many seconds before Succeeding<br>Timer is ticked inside the tre, no entities are spawned.                                                                          |
| DynamicSpawn | Spawns an entity when this node in the tree is reached, and waits for it to trigger a status report.<br>Once the entity triggers a status report, it is immediately despawned. |

### Unimplemented but possibly useful Task Nodes:

| Node   | Description                                                                                                                                                                                              |
| ------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Entity | When this node on the tree is reached, a `BehaveCtx` is inserted.<br>The tree then waits for this entity to trigger a status report.<br>On completion, `BehaveCtx` is removed, but nothing is despawned. |


## How conditionals work

I'm using observer events to implement conditionals. You specify an arbitrary struct which is 
delivered in a generic trigger which also carries a `BehaveTriggerCtx` value.

```rust
// Conditionals are types that are delivered by a trigger:
#[derive(Clone)]
struct HeightCheck {
    min_height: f32,
}

// add a global observer to answer conditional queries for HeightCheck:
app.add_observer(on_height_check);

// you respond by triggering a success or failure event created by the ctx:
fn on_height_check(trigger: Trigger<BehaveCondition<HeightCheck>>, q: Query<&Position>, mut commands: Commands) {
    let ctx: BehaveTriggerCtx = trigger.event().ctx();
    let height_check: HeightCheck = trigger.event().value();
    // lookup the position of the target entity (ie the entity this behaviour tree is controlling)
    let character_pos = q.get(ctx.target_entity()).expect("Character entity missing?");
    if character_pos.y >= height_check.min_height {
        commands.trigger(ctx.success());
    } else {
        commands.trigger(ctx.failure());
    }
}

// a behaviour tree that spawns an entity with `FlyAction` if the character is high enough:
let tree = tree! {
    Behave::conditional(HeightCheck{min_height: 100.0}) => {
        Behave::dynamic_spawn((Name:new("Take Off"), FlyAction::new(..))),
    }
}

```

<details>

<summary>Alternative approach taking `IntoSystem`</summary>

### Alternative approach for conditionals

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
it from an asset file for manipulation (or can you?)

I would also need mutable World in the "tick trees" system, which would stop it running in parallel maybe.

</details>
