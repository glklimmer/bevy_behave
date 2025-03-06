// NB: you can println!("{}", tree); and run the test like this to see output:
// cargo test -- --nocapture test_at_list
use crate::prelude::*;

/// Tests using the @ [] syntax for including a list of task nodes,
/// eg Behave::spawn_named or Wait etc – nothing that has children.
#[test]
fn test_at_list() {
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

/// Tests using the @ syntax to insert a single subtree
#[test]
fn test_at_tree() {
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

/// Shows how to use the ego_tree API to build a tree,
/// and then shows how to use the `...` syntax to append a list of subtrees.
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
    root.append(Behave::Wait(0.1));
    for subtree in trees.clone() {
        root.append_subtree(subtree);
    }
    assert_tree(
        "Sequence
            ├── Wait(0.1s)
            ├── Wait(1s)
            └── Sequence
                ├── Wait(1s)
                └── Wait(2s)",
        tree.clone(),
    );

    // the ... syntax appends a list of subtrees, so this creates the same tree as above:
    let t2 = behave! {
        Behave::Sequence => {
            Behave::Wait(0.1),
            ... trees,
        }
    };

    assert_eq!(tree.to_string(), t2.to_string());
}

/// asserts the tree.to_string matches the expected string, accounting for whitespace/indentation
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
