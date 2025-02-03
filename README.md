```rust
let tree = [
 FallbackFlow::with_children([
  Sequence::new("orbiting").with_children([ // seqflow
        // take a bundle to delay spawning?
  	Action::spawn((Name("orbit"), OrbitKeeperExec::new(..)))
    Action::wait(duration),
  	Action::invert(Action::spawn(((TargetRel..)))
  ]),
  Action::new((ExplodeExec)), // or explode on failure
 ]);
```
Action nodes need to store a dynamicentity for later spawning?

lose the ability to chuck components like "Trigger success after X mins" onto flow control nodes, since they aren't entities. but entity for current state of tree can access a FlowCtx component we insert?

spawn that onto one entity to manage the tree, so the logic of a bt is on an entity.
the bt entity spawns a single child (or more if parallel feature?) at a time.
OnRun doesn't exist, because it's Trigger<OnAdd, ActionComp> as the entity is spawned.
reporting results can be by triggering an event, and the bt entity observes. 
it would only ever need one observer per active child (usually 1).
reporting a result would make the bt entity despawn you.


Actions need to eventually report a Result to their parent.

If Action::spawn, the entity should trigger a result, and then we catch that and despawn it.

If Action::wait, we handle the delay

If Action::condition(system) we run the system and the output is the result? ie you can
have a bevy trigger/system like test that doesn't require a spawned entity.

Would still need access to the ctx to get the agent entity.


```rust
// test if the agent is in orbit around the given attractor?
Action::test(in_orbit(attractor_entity)) 
```






 
  