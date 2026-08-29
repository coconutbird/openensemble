use super::*;

#[test]
fn allow_veterancy_matches_retail_missing_and_present_defaults() {
    for (xml, expected) in [
        ("<Scenario />", false),
        ("<Scenario><AllowVeterancy /></Scenario>", true),
        (
            "<Scenario><AllowVeterancy>true</AllowVeterancy></Scenario>",
            true,
        ),
        (
            "<Scenario><AllowVeterancy>1</AllowVeterancy></Scenario>",
            true,
        ),
        (
            "<Scenario><AllowVeterancy>false</AllowVeterancy></Scenario>",
            false,
        ),
        (
            "<Scenario><AllowVeterancy>0</AllowVeterancy></Scenario>",
            false,
        ),
    ] {
        let document = Document::from_xml(xml).expect("valid scenario XML");
        assert_eq!(allows_veterancy(&document), expected, "{xml}");
    }
}

#[test]
fn visual_variation_map_retains_only_explicit_valid_placement_values() {
    let document = Document::from_xml(
        r#"<Scenario><Objects>
            <Object ID="10" VisualVariationIndex="2">marine</Object>
            <Object ID="11">random</Object>
            <Object ID="12" VisualVariationIndex="-2">negative</Object>
            <Object ID="bad" VisualVariationIndex="3">bad_id</Object>
            <Object ID="13" VisualVariationIndex="bad">bad_index</Object>
        </Objects></Scenario>"#,
    )
    .expect("valid scenario XML");

    assert_eq!(
        visual_variation_indices(&document)
            .into_iter()
            .collect::<Vec<_>>(),
        vec![(10, 2), (12, -1)]
    );
}
