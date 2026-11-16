#[path = "support/task_pool_fixture.rs"]
mod task_pool_fixture;
use nexora_platform::*;
use task_pool_fixture::TaskPoolFixture;
#[test]
#[ignore = "requires PostgreSQL; alphabetic ordering precedes pagination in both directions"]
fn task_pages_are_alphabetic_and_reverse_without_missing_equal_titles() {
    let mut f = TaskPoolFixture::new(false);
    for title in ["Zulu", "Alpha", "Bravo", "Alpha"] {
        f.work
            .store
            .create_pool_task(
                &f.manager,
                &PoolTaskCreate {
                    project_id: None,
                    definition: TaskCreate {
                        title: title.into(),
                        type_code: "task".into(),
                        origin: "test".into(),
                        ..Default::default()
                    },
                },
            )
            .unwrap();
    }
    for descending in [false, true] {
        let mut query = VisibleTaskQuery {
            limit: 1,
            descending,
            ..Default::default()
        };
        let mut names = Vec::new();
        let mut ids = std::collections::HashSet::new();
        loop {
            let page = f.work.store.visible_tasks(&f.manager, &query).unwrap();
            for item in page.items {
                assert!(ids.insert(item.task.id));
                names.push(item.task.title)
            }
            if page.next_after.is_none() {
                break;
            }
            query.after = page.next_after;
            assert!(names.len() < 5);
        }
        let expected = if descending {
            vec!["Zulu", "Bravo", "Alpha", "Alpha"]
        } else {
            vec!["Alpha", "Alpha", "Bravo", "Zulu"]
        };
        assert_eq!(names, expected);
    }
}
