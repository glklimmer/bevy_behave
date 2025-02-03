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

| Node         | Description                                                                                                                                                                    |
| ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Wait         | Waits this many seconds before Succeeding<br>Timer is ticked inside the tre, no entities are spawned.                                                                          |
| DynamicSpawn | Spawns an entity when this node in the tree is reached, and waits for it to trigger a status report.<br>Once the entity triggers a status report, it is immediately despawned. |
| Entity       | When this node on the tree is reached, a `BehaveCtx` is inserted.<br>The tree then waits for this entity to trigger a status report.<br>No automatic despawning.               |

