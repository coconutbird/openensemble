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
