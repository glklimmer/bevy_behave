use crate::prelude::*;

// NB: you can println!("{}", tree); and run the test like this to see output:
// cargo test -- --nocapture test_at_list

#[test]
fn test_at_list() {
    // the @ [] syntax is for including a list of task nodes, ie Behave::spawn_named or Wait etc.
    // nothing that has children.
    let behaviours = [Behave::Wait(1.0), Behave::Wait(2.0), Behave::Wait(3.0)];
    let tree = behave! {
        Behave::Sequence => {
            Behave::Wait(5.0),
            @[ behaviours ]
        }
    };
    assert_tree(
        "Sequence
            ├── Wait(5s)
            ├── Wait(1s)
            ├── Wait(2s)
            └── Wait(3s)",
        tree,
    );
}

#[test]
fn test_at_tree() {
    // the @ syntax is for inserting a single subtree
    let subtree = behave! {
        Behave::Sequence => {
            Behave::Wait(1.0),
            Behave::Wait(2.0),
        }
    };
    let tree = behave! {
        Behave::Sequence => {
            Behave::Wait(5.0),
            @ subtree
        }
    };
    assert_tree(
        "Sequence
            ├── Wait(5s)
            └── Sequence
                ├── Wait(1s)
                └── Wait(2s)",
        tree,
    );
}

#[test]
fn test_ego_tree_api() {
    let trees = [
        behave! {
            Behave::Wait(1.0),
        },
        behave! {
            Behave::Sequence => {
                Behave::Wait(1.0),
                Behave::Wait(2.0),
            }
        },
    ];
    let mut tree = ego_tree::Tree::new(Behave::Sequence);
    let mut root = tree.root_mut();
    for subtree in trees {
        root.append_subtree(subtree);
    }
    assert_tree(
        "Sequence
            ├── Wait(1s)
            └── Sequence
                ├── Wait(1s)
                └── Wait(2s)",
        tree,
    );
}

fn assert_tree(s: &str, tree: Tree<Behave>) {
    // strip and tidy any indent spaces in the expected output so we can easily compare
    let leading_spaces = s
        .lines()
        .find(|line| !line.trim().is_empty() && line.starts_with(' '))
        .map(|line| line.len() - line.trim_start().len())
        .unwrap_or(0);
    let mut expected = s
        .lines()
        .map(|line| {
            if line.len() >= leading_spaces {
                &line[leading_spaces..]
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    expected.push('\n');
    assert_eq!(tree.to_string(), expected);
}
