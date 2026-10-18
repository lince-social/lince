use super::*;

#[test]
fn hidden_withdrawal_never_restores_an_old_path_or_discloses_its_revision() {
    support::run_async_test("contact-hidden-withdrawal", || async {
        let node = support::engine().await;
        let a = support::person(&node, "contact.hidden.a").await;
        let b = support::person(&node, "contact.hidden.b").await;
        authorize(&node, &a).await;
        authorize(&node, &b).await;
        let reviewed = preview(
            &node,
            &a,
            draft(&a, &b, 0, false),
            "contact:visible:preview",
        )
        .await;
        let first = publish(&node, &a, &reviewed, "contact:visible:publish").await;
        let mut policy = paths::Policy::for_anchor(Identity::Person(a.uid.clone()));
        policy.allow_unilateral = true;
        let before = node
            .act(
                action(
                    &a,
                    "contact:visible:paths",
                    Command::ContactPaths {
                        policy: policy.clone(),
                        as_organ: false,
                    },
                ),
                Some(a.uid.clone()),
            )
            .await
            .unwrap()
            .data
            .unwrap();
        assert_eq!(
            before["trust"]["result"]["paths"].as_array().unwrap().len(),
            1
        );
        let reviewed = preview(&node, &a, draft(&a, &b, 1, true), "contact:hidden:preview").await;
        let hidden = publish(&node, &a, &reviewed, "contact:hidden:publish").await;
        node.act(
            Action::Visibility {
                request: nucleus::visibility::Command::Save {
                    record_uid: hidden.clone(),
                    data: nucleus::visibility::Data::Record,
                    policy: nucleus::visibility::Policy::default(),
                    expected_revision: 1,
                },
            },
            None,
        )
        .await
        .unwrap();
        let after = node
            .act(
                action(
                    &a,
                    "contact:hidden:paths",
                    Command::ContactPaths {
                        policy,
                        as_organ: false,
                    },
                ),
                Some(a.uid.clone()),
            )
            .await
            .unwrap()
            .data
            .unwrap();
        assert!(
            after["trust"]["result"]["paths"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(!after.to_string().contains(&hidden));
        let listing = node
            .act(
                action(
                    &a,
                    "contact:hidden:list",
                    Command::ListContacts {
                        before: None,
                        as_organ: false,
                    },
                ),
                Some(a.uid.clone()),
            )
            .await
            .unwrap()
            .data
            .unwrap();
        let entries = listing["trust"]["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["record"], first);
        assert_eq!(entries[0]["current_revision"], Value::Null);
        assert!(!listing.to_string().contains(&hidden));
    });
}
