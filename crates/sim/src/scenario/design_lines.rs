//! Loading of retail `DesignObjects/Lines` path geometry.

use super::scenario_object_position_to_world;
use crate::world::{DesignLineId, World};
use glam::Vec3;
use pipeline::xmb::{Document, Node};

/// Failure while parsing one scenario design line.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DesignLineLoadError {
    /// A line did not supply the retail integer identity used by triggers.
    #[error("a design line is missing its ID attribute")]
    MissingId,
    /// A line supplied an ID that was not a signed retail integer.
    #[error("design-line ID '{value}' is not a valid integer")]
    InvalidId { value: String },
    /// A position or path point did not contain three finite floats.
    #[error("design line {line_id} has invalid {field} vector '{value}'")]
    InvalidVector {
        line_id: DesignLineId,
        field: &'static str,
        value: String,
    },
}

pub(super) fn load_design_lines(
    world: &mut World,
    document: &Document,
) -> Result<(), DesignLineLoadError> {
    let lines = parse_design_lines(document)?;
    world.configure_design_lines(lines);
    Ok(())
}

fn parse_design_lines(
    document: &Document,
) -> Result<Vec<(DesignLineId, Vec<Vec3>)>, DesignLineLoadError> {
    let Some(wrapper) = document
        .root()
        .and_then(|root| child(root, "DesignObjects"))
        .and_then(|objects| child(objects, "Lines"))
    else {
        return Ok(Vec::new());
    };
    wrapper.children.iter().map(parse_design_line).collect()
}

fn parse_design_line(node: &Node) -> Result<(DesignLineId, Vec<Vec3>), DesignLineLoadError> {
    let id_attribute = node
        .get_attribute("ID")
        .ok_or(DesignLineLoadError::MissingId)?;
    let id_text = id_attribute.value_string();
    let line_id = id_text
        .trim()
        .parse()
        .map_err(|_| DesignLineLoadError::InvalidId { value: id_text })?;
    let mut points = Vec::new();
    if let Some(position) = node.get_attribute("Position") {
        points.push(parse_point(line_id, "Position", &position.value_string())?);
    }
    for points_node in node.children.iter().filter(|node| node.name == "Points") {
        for point in points_node
            .text_string()
            .split('|')
            .map(str::trim)
            .filter(|point| !point.is_empty())
        {
            points.push(parse_point(line_id, "Points", point)?);
        }
    }
    Ok((line_id, points))
}

fn parse_point(
    line_id: DesignLineId,
    field: &'static str,
    value: &str,
) -> Result<Vec3, DesignLineLoadError> {
    let invalid = || DesignLineLoadError::InvalidVector {
        line_id,
        field,
        value: value.to_owned(),
    };
    let mut components = value.split(',').map(str::trim);
    let parse_component = |component: Option<&str>| {
        component
            .and_then(|component| component.parse::<f32>().ok())
            .filter(|component| component.is_finite())
            .ok_or_else(&invalid)
    };
    let authored = [
        parse_component(components.next())?,
        parse_component(components.next())?,
        parse_component(components.next())?,
    ];
    if components.next().is_some() {
        return Err(invalid());
    }
    Ok(Vec3::from_array(scenario_object_position_to_world(
        authored,
    )))
}

fn child<'a>(node: &'a Node, name: &str) -> Option<&'a Node> {
    node.children.iter().find(|child| child.name == name)
}

#[cfg(test)]
mod tests;
