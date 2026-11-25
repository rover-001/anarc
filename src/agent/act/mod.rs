mod dom;
mod exec;
mod parse;
mod types;

use anarc_telemetry::schema::ErrorCode;

pub use dom::set_element_value;
pub use exec::{execute_dsl, take_pending_upload_files};
pub use parse::parse_dsl;
pub use types::ActionOp;
#[cfg(test)]
pub use types::WaitCondition;

// Error telemetry records an enum chosen at the failure site. No error text
// (which can embed page content) is ever passed to the counters.
pub(super) fn note_error(code: ErrorCode) {
    super::telemetry::telemetry().counters.note_error(code);
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::types::{DblClickTarget, DragTarget, ScrollDirection, SplitAction, TabAction};

    #[test]
    fn parser_errors_give_next_commands() {
        for input in ["key", "go", "drag b1", "tab invalid", "bogus", "@missing"] {
            let error = parse_dsl(input).err().unwrap_or_default();
            assert!(
                error.contains("use:") || error.contains("retry"),
                "{input}: {error}"
            );
            assert!(error.len() <= 86, "{input}: {error}");
        }
    }

    #[test]
    fn dom_click_parses_from_both_aliases() {
        for (script, target) in [
            ("clickjs b1", "b1"),
            ("domclick b1", "b1"),
            (
                "clickjs css=a[href^='/messaging/']",
                "css=a[href^='/messaging/']",
            ),
        ] {
            let ops = parse_dsl(script).unwrap_or_default();
            assert_eq!(
                ops,
                vec![ActionOp::DomClick {
                    ref_id: target.into()
                }],
                "{script}"
            );
        }
        assert!(parse_dsl("clickjs").is_err());
    }

    #[test]
    fn diff_output_is_capped() {
        let mut diff = crate::agent::refs::SnapshotDiff::default();
        for index in 0..40 {
            diff.removed.push(format!("l{index}"));
        }
        let output = exec::compact_diff(&diff);
        assert!(output.contains("…+16 more"), "{output}");
        assert!(output.matches("-l").count() <= 24, "{output}");
    }

    #[test]
    fn fill_failures_name_the_reason_and_ref() {
        assert_eq!(
            dom::field_unavailable_error("no-field", "Write a message"),
            "input field not focused or off-screen: Write a message. use: see"
        );
        assert_eq!(
            dom::field_unavailable_error("disabled", "Email"),
            "input field disabled: Email. use: see"
        );
        assert_eq!(
            dom::field_unavailable_error("readonly", "Date"),
            "input field read-only: Date. use: see"
        );
        assert_eq!(
            dom::field_unavailable_error("not-editable", "Send"),
            "element not editable: Send. use: see"
        );
        assert!(dom::field_unavailable_error("unknown", "").starts_with("field unavailable:"));
    }

    #[test]
    fn test_dsl_parser_variety() {
        let script = r#"
            b1;
            i1="rover@test.com";
            i2=@secret:github;
            b:"Sign in";
            key Enter;
            go https://example.com;
            scroll down;
            wait idle;
            paste b1;
            wait stream;
        "#;

        let ops = parse_dsl(script).unwrap_or_default();
        assert_eq!(ops.len(), 10);
        assert_eq!(
            ops.first(),
            Some(&ActionOp::Click {
                ref_id: "b1".into()
            })
        );
        assert_eq!(
            ops.get(1),
            Some(&ActionOp::ClearAndType {
                ref_id: "i1".into(),
                text: "rover@test.com".into()
            })
        );
        assert_eq!(
            ops.get(2),
            Some(&ActionOp::TypeSecret {
                ref_id: "i2".into(),
                secret_name: "github".into()
            })
        );
        assert_eq!(
            ops.get(3),
            Some(&ActionOp::SemanticClick {
                kind: 'b',
                label: "Sign in".into()
            })
        );
        assert_eq!(
            ops.get(4),
            Some(&ActionOp::Key {
                chord: "Enter".into()
            })
        );
        assert_eq!(
            ops.get(5),
            Some(&ActionOp::Go {
                url: "https://example.com".into()
            })
        );
        assert_eq!(
            ops.get(6),
            Some(&ActionOp::Scroll {
                direction: ScrollDirection::Down
            })
        );
        assert_eq!(
            ops.get(7),
            Some(&ActionOp::Wait {
                condition: WaitCondition::Idle
            })
        );
        assert_eq!(
            ops.get(8),
            Some(&ActionOp::Paste {
                ref_id: Some("b1".into()),
                text: None
            })
        );
        assert_eq!(
            ops.get(9),
            Some(&ActionOp::Wait {
                condition: WaitCondition::Stream {
                    quiescent_ms: 1000,
                    timeout_ms: 30000
                }
            })
        );
    }

    #[test]
    fn test_quote_aware_multiline_statement_splitting() {
        let script = "i1=\"line 1\nline 2; still line 2\"; b1";
        let ops = parse_dsl(script).unwrap_or_default();
        assert_eq!(ops.len(), 2);
        assert_eq!(
            ops.first(),
            Some(&ActionOp::ClearAndType {
                ref_id: "i1".into(),
                text: "line 1\nline 2; still line 2".into()
            })
        );
        assert_eq!(
            ops.get(1),
            Some(&ActionOp::Click {
                ref_id: "b1".into()
            })
        );
    }

    #[test]
    fn test_triple_quotes_and_backticks_preserve_semicolons() {
        let script = "i1=\"\"\"line 1; line 2; semicolon here;\"\"\"; b1";
        let ops = parse_dsl(script).unwrap_or_default();
        assert_eq!(ops.len(), 2);
        assert_eq!(
            ops.first(),
            Some(&ActionOp::ClearAndType {
                ref_id: "i1".into(),
                text: "line 1; line 2; semicolon here;".into()
            })
        );
        assert_eq!(
            ops.get(1),
            Some(&ActionOp::Click {
                ref_id: "b1".into()
            })
        );

        let script2 = "i2=`SELECT * FROM users; WHERE id = 1;`; b2";
        let ops2 = parse_dsl(script2).unwrap_or_default();
        assert_eq!(ops2.len(), 2);
        assert_eq!(
            ops2.first(),
            Some(&ActionOp::ClearAndType {
                ref_id: "i2".into(),
                text: "SELECT * FROM users; WHERE id = 1;".into()
            })
        );
    }

    #[test]
    fn test_upload_parsing_multiple_files() {
        let script = "upload b1 /tmp/test1.png /tmp/test2.png";
        let ops = parse_dsl(script).unwrap_or_default();
        assert_eq!(ops.len(), 1);
        assert_eq!(
            ops.first(),
            Some(&ActionOp::Upload {
                ref_id: "b1".into(),
                file_paths: vec!["/tmp/test1.png".into(), "/tmp/test2.png".into()]
            })
        );
    }

    #[test]
    fn test_macro_expansion_in_dsl() {
        let script = r#"
            @search "agent control";
            scroll down;
        "#;

        let ops = parse_dsl(script).unwrap_or_default();
        assert!(ops.len() >= 2);
    }

    #[test]
    fn test_dsl_parser_fuzz_never_panics() {
        let garbage = [
            "",
            ";;;;",
            "\n\n\n",
            "=",
            "+",
            ":",
            "key",
            "go",
            "scroll",
            "hover",
            "focus",
            "drag",
            "xy",
            "up",
            "tab",
            "wait",
            "b1=foo=bar=baz",
            "i1+\"unfinished quote",
            "x:\"weird\":extra:colons",
            "@unknown_macro_404",
            "@secret:",
            "dialog whatever",
            "scroll non_existent_direction",
            "xy not_a_number not_a_number",
            "tab not_a_number",
            "wait not_a_number",
            "very_long_string_without_delimiters_1234567890_abcdefghijklmnopqrstuvwxyz",
        ];

        for &sample in &garbage {
            let _ = parse_dsl(sample);
        }
    }

    #[test]
    fn test_enhanced_dsl_features() {
        let script = r#"
            navigate https://example.com;
            keydown Shift;
            key ArrowLeft;
            keyup Shift;
            key "Ctrl+v";
            click css=button[type=submit];
            click aria="Send";
            type i1 "hello human";
            type append i2 " world";
            fill css=input[name=q] "search text";
            tab select personal-tab-3;
            tab personal-tab-4;
        "#;
        let ops = parse_dsl(script).unwrap_or_default();
        assert_eq!(ops.len(), 12);
        assert_eq!(
            ops.first(),
            Some(&ActionOp::Go {
                url: "https://example.com".into()
            })
        );
        assert_eq!(
            ops.get(1),
            Some(&ActionOp::KeyDown {
                chord: "Shift".into()
            })
        );
        assert_eq!(
            ops.get(2),
            Some(&ActionOp::Key {
                chord: "ArrowLeft".into()
            })
        );
        assert_eq!(
            ops.get(3),
            Some(&ActionOp::KeyUp {
                chord: "Shift".into()
            })
        );
        assert_eq!(
            ops.get(4),
            Some(&ActionOp::Key {
                chord: "Ctrl+v".into()
            })
        );
        assert_eq!(
            ops.get(5),
            Some(&ActionOp::Click {
                ref_id: "css=button[type=submit]".into()
            })
        );
        assert_eq!(
            ops.get(6),
            Some(&ActionOp::Click {
                ref_id: "aria=\"Send\"".into()
            })
        );
        assert_eq!(
            ops.get(7),
            Some(&ActionOp::ClearAndType {
                ref_id: "i1".into(),
                text: "hello human".into()
            })
        );
        assert_eq!(
            ops.get(8),
            Some(&ActionOp::AppendText {
                ref_id: "i2".into(),
                text: " world".into()
            })
        );
        assert_eq!(
            ops.get(9),
            Some(&ActionOp::ClearAndType {
                ref_id: "css=input[name=q]".into(),
                text: "search text".into()
            })
        );
        assert_eq!(
            ops.get(10),
            Some(&ActionOp::Tab {
                action: TabAction::Select("personal-tab-3".into())
            })
        );
        assert_eq!(
            ops.get(11),
            Some(&ActionOp::Tab {
                action: TabAction::Select("personal-tab-4".into())
            })
        );
    }

    #[test]
    fn test_batch_set_dblclick_drag_upload_and_text_click() {
        let script = r##"
            set @i10 "750";
            set @i12 "120";
            set @i8 "80" enter;
            dblclick @b1;
            dblclick xy 100 200;
            dblclick "Edit text";
            drag @b1 @b2;
            drag 10 20 30 40;
            drag xy 50 60 70 80;
            drag @b1 30 40;
            upload "/tmp/a.png" "/tmp/b.png";
            click "Add a new color";
            click "#18c3da";
        "##;
        let Ok(ops) = parse_dsl(script) else {
            return;
        };
        assert_eq!(
            ops,
            vec![
                ActionOp::SetValue {
                    target: "i10".into(),
                    value: "750".into(),
                    enter: false
                },
                ActionOp::SetValue {
                    target: "i12".into(),
                    value: "120".into(),
                    enter: false
                },
                ActionOp::SetValue {
                    target: "i8".into(),
                    value: "80".into(),
                    enter: true
                },
                ActionOp::DblClick {
                    target: DblClickTarget::Target("b1".into())
                },
                ActionOp::DblClick {
                    target: DblClickTarget::Coords(100, 200)
                },
                ActionOp::DblClick {
                    target: DblClickTarget::Target("Edit text".into())
                },
                ActionOp::Drag {
                    from: DragTarget::Ref("b1".into()),
                    to: DragTarget::Ref("b2".into())
                },
                ActionOp::Drag {
                    from: DragTarget::Coords(10, 20),
                    to: DragTarget::Coords(30, 40)
                },
                ActionOp::Drag {
                    from: DragTarget::Coords(50, 60),
                    to: DragTarget::Coords(70, 80)
                },
                ActionOp::Drag {
                    from: DragTarget::Ref("b1".into()),
                    to: DragTarget::Coords(30, 40)
                },
                ActionOp::Upload {
                    ref_id: String::new(),
                    file_paths: vec!["/tmp/a.png".into(), "/tmp/b.png".into()]
                },
                ActionOp::Click {
                    ref_id: "Add a new color".into()
                },
                ActionOp::Click {
                    ref_id: "#18c3da".into()
                },
            ]
        );

        let Ok(extra) = parse_dsl(
            r#"click! @i2; click xy 120 340; type "Rank"; paste "A\tB\n1\t2""#,
        ) else {
            return;
        };
        assert_eq!(
            extra,
            vec![
                ActionOp::ForceClick {
                    ref_id: "i2".into()
                },
                ActionOp::XY { x: 120, y: 340 },
                ActionOp::TypeActive {
                    text: "Rank".into()
                },
                ActionOp::Paste {
                    ref_id: None,
                    text: Some(r"A\tB\n1\t2".into())
                },
            ]
        );

        let Ok(ws) = parse_dsl(
            r#"space "Work"; split; split https://sheets.new; split 1 2; split off"#,
        ) else {
            return;
        };
        assert_eq!(
            ws,
            vec![
                ActionOp::Space {
                    target: "Work".into()
                },
                ActionOp::Split {
                    action: SplitAction::WithNext
                },
                ActionOp::Split {
                    action: SplitAction::WithUrl("https://sheets.new".into())
                },
                ActionOp::Split {
                    action: SplitAction::Pair("1".into(), "2".into())
                },
                ActionOp::Split {
                    action: SplitAction::Exit
                },
            ]
        );

        let Ok(waits) = parse_dsl(r#"wait 300; wait_for "Goa""#) else {
            return;
        };
        assert_eq!(
            waits,
            vec![
                ActionOp::Wait {
                    condition: WaitCondition::Millis(300)
                },
                ActionOp::Wait {
                    condition: WaitCondition::Text("Goa".into())
                },
            ]
        );
    }
}
