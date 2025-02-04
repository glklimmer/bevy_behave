# bevy_behave examples

## Chase

Each enemy entity gets a BehaveTree child entity. and in the normal case, those entities are using
another entity to run the `WaitUntilPlayerIsNear` behaviour. **So 1 enemy = 3 entities.**

Use **release mode** if you want to spawn lots of enemies at once!


<pre>
cargo run --release --example chase
</pre>

<img src="https://github.com/RJ/bevy_behave/blob/main/examples/bevy_behave_chase_example.png">