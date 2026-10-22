use super::*;
use pipeline::hw1::scenario::ScenarioData;

#[test]
fn visual_variation_map_retains_only_explicit_placement_values() {
    let scenario = ScenarioData::from_xml_str(
        r#"<Scenario><Objects>
            <Object ID="10" VisualVariationIndex="2">marine</Object>
            <Object ID="11">random</Object>
            <Object ID="12" VisualVariationIndex="-2">negative</Object>
            <Object ID="13" VisualVariationIndex="bad">bad_index</Object>
            <Object ID="14" VisualVariationIndex="0">first</Object>
            <Object ID="15" VisualVariationIndex="1">duplicate</Object>
            <Object ID="15" VisualVariationIndex="3">duplicate</Object>
            <Object ID="15">duplicate</Object>
        </Objects></Scenario>"#,
    )
    .expect("valid scenario XML");

    assert_eq!(
        visual_variation_indices(scenario.objects())
            .into_iter()
            .collect::<Vec<_>>(),
        vec![(10, 2), (12, -1), (14, 0), (15, 3)]
    );
}
