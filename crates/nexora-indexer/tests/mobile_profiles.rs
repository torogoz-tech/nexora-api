use nexora_core::{RepositoryId, TenantId};
use nexora_indexer::{
    AnalysisProfileCandidate, ExternalFileCache, MemoryStore, MultiProfileIndexer,
};
use nexora_language::AnalysisProfilePackage;
use std::{fs, path::Path};

#[test]
fn mobile_profiles_recognize_declarations_without_promoting_comments_or_strings() {
    for (language, extension, source, expected) in [
        (
            "dart",
            "dart",
            "import 'package:flutter/widgets.dart';\n// class Phantom {}\nclass Home extends StatelessWidget { Widget build(BuildContext context) => Text('class Fake {}'); }\nString greeting() { return '''class Imaginary {}'''; }",
            vec!["Home", "build", "greeting"],
        ),
        (
            "swift",
            "swift",
            "import Foundation\n// class Phantom {}\nstruct Home { func render() -> String { return \"class Fake {}\" } }\nfunc greeting() { print(\"Hi\") }",
            vec!["Home", "render", "greeting"],
        ),
        (
            "kotlin",
            "kt",
            "import android.app.Activity\n// class Phantom {}\nclass Home : Activity() { fun render(): String { return \"\"\"class Fake {}\"\"\" } }\nfun greeting() { println(\"Hi\") }",
            vec!["Home", "render", "greeting"],
        ),
        (
            "java",
            "java",
            "import android.app.Activity;\n// class Phantom {}\npublic class Home extends Activity { public String render() { return \"class Fake {}\"; } }",
            vec!["Home", "render"],
        ),
    ] {
        let root = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(format!("nexora-mobile-{}-{language}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join(format!("home.{extension}")), source).unwrap();
        let package = AnalysisProfilePackage::from_catalog(
            Path::new("../../templates/mobile-understanding"),
            language.into(),
            format!("mobile-{language}"),
            if ["dart", "kotlin"].contains(&language) {
                "0.1.1"
            } else {
                "0.1.0"
            }
            .into(),
            language.into(),
        )
        .unwrap();
        let candidate = AnalysisProfileCandidate {
            id: language.into(),
            checksum: package.checksum().unwrap(),
            created_at: "2026-11-10T00:00:00.000000Z".into(),
            package,
        };
        let indexer = MultiProfileIndexer::new(vec![candidate], true).unwrap();
        let mut read = |_: &nexora_indexer::FileCacheKey| Ok(None);
        let mut write =
            |_: &nexora_indexer::FileCacheKey, _: &nexora_indexer::CacheFragment| Ok(false);
        let result = indexer
            .index_repository_external(
                &root,
                TenantId("tenant".into()),
                RepositoryId("repo_mobile".into()),
                &mut MemoryStore::default(),
                &mut ExternalFileCache {
                    read: &mut read,
                    write: &mut write,
                },
            )
            .unwrap();
        assert!(
            result.index.snapshot.diagnostics.is_empty(),
            "{language}: {:?}",
            result.index.snapshot.diagnostics
        );
        assert!(
            result
                .index
                .snapshot
                .graph
                .nodes
                .iter()
                .any(|node| node.kind == nexora_graph::NodeKind::Module),
            "{language}: import missing"
        );
        let names: Vec<_> = result
            .index
            .snapshot
            .graph
            .nodes
            .iter()
            .filter(|node| {
                matches!(
                    node.kind,
                    nexora_graph::NodeKind::Class
                        | nexora_graph::NodeKind::Function
                        | nexora_graph::NodeKind::Method
                )
            })
            .map(|node| node.name.as_str())
            .collect();
        for name in expected {
            assert!(
                names.contains(&name),
                "{language}: missing {name}, found {names:?}"
            );
        }
        for phantom in ["Phantom", "Fake", "Imaginary"] {
            assert!(
                !names.contains(&phantom),
                "{language}: false declaration {phantom}"
            );
        }
        assert_eq!(result.index.snapshot.sources.len(), 1, "{language}");
        fs::remove_dir_all(root).unwrap();
    }
}
