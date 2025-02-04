# bevy_behave examples

## Chase

Each enemy entity gets a BehaveTree child entity. and in the normal case, those entities are using
another entity to run the `WaitUntilPlayerIsNear` behaviour. So 1 enemy = 3 entities.