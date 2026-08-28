//! Retail AI squad-analysis values shared by trigger effects.

use crate::sync::SyncChecksum;

pub(crate) const AI_DAMAGE_CLASS_COUNT: usize = 6;

/// The six base armor classes used by retail AI combat scoring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AIDamageClass {
    Light = 0,
    LightArmored = 1,
    Medium = 2,
    MediumAir = 3,
    Heavy = 4,
    Building = 5,
}

impl AIDamageClass {
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        match name.trim() {
            name if name.eq_ignore_ascii_case("Light") => Some(Self::Light),
            name if name.eq_ignore_ascii_case("LightArmored") => Some(Self::LightArmored),
            name if name.eq_ignore_ascii_case("Medium") => Some(Self::Medium),
            name if name.eq_ignore_ascii_case("MediumAir") => Some(Self::MediumAir),
            name if name.eq_ignore_ascii_case("Heavy") => Some(Self::Heavy),
            name if name.eq_ignore_ascii_case("Building") => Some(Self::Building),
            _ => None,
        }
    }

    pub(crate) const fn index(self) -> usize {
        self as usize
    }
}

/// Component selector stored by retail `AISquadAnalysisComponent` variables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum AISquadAnalysisComponent {
    #[default]
    Invalid = 0,
    CVLight = 1,
    CVLightArmored = 2,
    CVMedium = 3,
    CVMediumAir = 4,
    CVHeavy = 5,
    CVBuilding = 6,
    CVTotal = 7,
    HPLight = 8,
    HPLightArmored = 9,
    HPMedium = 10,
    HPMediumAir = 11,
    HPHeavy = 12,
    HPBuilding = 13,
    HPTotal = 14,
    SPLight = 15,
    SPLightArmored = 16,
    SPMedium = 17,
    SPMediumAir = 18,
    SPHeavy = 19,
    SPBuilding = 20,
    SPTotal = 21,
    DPSLight = 22,
    DPSLightArmored = 23,
    DPSMedium = 24,
    DPSMediumAir = 25,
    DPSHeavy = 26,
    DPSBuilding = 27,
    DPSTotal = 28,
    CVPercentLight = 29,
    CVPercentLightArmored = 30,
    CVPercentMedium = 31,
    CVPercentMediumAir = 32,
    CVPercentHeavy = 33,
    CVPercentBuilding = 34,
    HPPercentLight = 35,
    HPPercentLightArmored = 36,
    HPPercentMedium = 37,
    HPPercentMediumAir = 38,
    HPPercentHeavy = 39,
    HPPercentBuilding = 40,
    CVStarsLight = 41,
    CVStarsLightArmored = 42,
    CVStarsMedium = 43,
    CVStarsMediumAir = 44,
    CVStarsHeavy = 45,
    CVStarsBuilding = 46,
    CVStarsTotal = 47,
    NormalizedStarsLight = 48,
    NormalizedStarsLightArmored = 49,
    NormalizedStarsMedium = 50,
    NormalizedStarsMediumAir = 51,
    NormalizedStarsHeavy = 52,
    NormalizedStarsBuilding = 53,
}

impl AISquadAnalysisComponent {
    /// Resolve the exact editor/database spelling used in trigger XML.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        let normalized = name.trim().to_ascii_lowercase();
        Some(match normalized.as_str() {
            "invalid" => Self::Invalid,
            "cvlight" => Self::CVLight,
            "cvlightarmored" => Self::CVLightArmored,
            "cvmedium" => Self::CVMedium,
            "cvmediumair" => Self::CVMediumAir,
            "cvheavy" => Self::CVHeavy,
            "cvbuilding" => Self::CVBuilding,
            "cvtotal" => Self::CVTotal,
            "hplight" => Self::HPLight,
            "hplightarmored" => Self::HPLightArmored,
            "hpmedium" => Self::HPMedium,
            "hpmediumair" => Self::HPMediumAir,
            "hpheavy" => Self::HPHeavy,
            "hpbuilding" => Self::HPBuilding,
            "hptotal" => Self::HPTotal,
            "splight" => Self::SPLight,
            "splightarmored" => Self::SPLightArmored,
            "spmedium" => Self::SPMedium,
            "spmediumair" => Self::SPMediumAir,
            "spheavy" => Self::SPHeavy,
            "spbuilding" => Self::SPBuilding,
            "sptotal" => Self::SPTotal,
            "dpslight" => Self::DPSLight,
            "dpslightarmored" => Self::DPSLightArmored,
            "dpsmedium" => Self::DPSMedium,
            "dpsmediumair" => Self::DPSMediumAir,
            "dpsheavy" => Self::DPSHeavy,
            "dpsbuilding" => Self::DPSBuilding,
            "dpstotal" => Self::DPSTotal,
            "cvpercentlight" => Self::CVPercentLight,
            "cvpercentlightarmored" => Self::CVPercentLightArmored,
            "cvpercentmedium" => Self::CVPercentMedium,
            "cvpercentmediumair" => Self::CVPercentMediumAir,
            "cvpercentheavy" => Self::CVPercentHeavy,
            "cvpercentbuilding" => Self::CVPercentBuilding,
            "hppercentlight" => Self::HPPercentLight,
            "hppercentlightarmored" => Self::HPPercentLightArmored,
            "hppercentmedium" => Self::HPPercentMedium,
            "hppercentmediumair" => Self::HPPercentMediumAir,
            "hppercentheavy" => Self::HPPercentHeavy,
            "hppercentbuilding" => Self::HPPercentBuilding,
            "cvstarslight" => Self::CVStarsLight,
            "cvstarslightarmored" => Self::CVStarsLightArmored,
            "cvstarsmedium" => Self::CVStarsMedium,
            "cvstarsmediumair" => Self::CVStarsMediumAir,
            "cvstarsheavy" => Self::CVStarsHeavy,
            "cvstarsbuilding" => Self::CVStarsBuilding,
            "cvstarstotal" => Self::CVStarsTotal,
            "normalizedstarslight" => Self::NormalizedStarsLight,
            "normalizedstarslightarmored" => Self::NormalizedStarsLightArmored,
            "normalizedstarsmedium" => Self::NormalizedStarsMedium,
            "normalizedstarsmediumair" => Self::NormalizedStarsMediumAir,
            "normalizedstarsheavy" => Self::NormalizedStarsHeavy,
            "normalizedstarsbuilding" => Self::NormalizedStarsBuilding,
            _ => return None,
        })
    }
}

/// Aggregate combat makeup and offensive capability for a squad list.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AISquadAnalysis {
    combat_values: [f32; AI_DAMAGE_CLASS_COUNT],
    hitpoints: [f32; AI_DAMAGE_CLASS_COUNT],
    shieldpoints: [f32; AI_DAMAGE_CLASS_COUNT],
    damage_per_second: [f32; AI_DAMAGE_CLASS_COUNT],
    combat_value_percent: [f32; AI_DAMAGE_CLASS_COUNT],
    hitpoint_percent: [f32; AI_DAMAGE_CLASS_COUNT],
    combat_value_stars: [f32; AI_DAMAGE_CLASS_COUNT],
    normalized_stars: [f32; AI_DAMAGE_CLASS_COUNT],
    combat_value_total: f32,
    hitpoint_total: f32,
    shieldpoint_total: f32,
    damage_per_second_total: f32,
    combat_value_stars_total: f32,
}

impl AISquadAnalysis {
    pub(crate) fn add_squad(
        &mut self,
        class: Option<AIDamageClass>,
        combat_value_hp: f32,
        hitpoints: f32,
        shieldpoints: f32,
        base_damage_per_second: f32,
        attack_ratings: [f32; AI_DAMAGE_CLASS_COUNT],
    ) {
        if let Some(class) = class {
            let class = class.index();
            self.combat_values[class] += combat_value_hp;
            self.hitpoints[class] += hitpoints;
            self.shieldpoints[class] += shieldpoints;
        }
        for (index, rating) in attack_ratings.into_iter().enumerate() {
            self.damage_per_second[index] += rating;
            let stars = if base_damage_per_second == 0.0 || rating == 0.0 {
                0.0
            } else {
                rating / base_damage_per_second
            };
            self.combat_value_stars[index] += combat_value_hp * stars;
        }
    }

    pub(crate) fn finish(&mut self) {
        self.combat_value_total = self.combat_values.iter().sum();
        self.hitpoint_total = self.hitpoints.iter().sum();
        self.shieldpoint_total = self.shieldpoints.iter().sum();
        self.damage_per_second_total = self.damage_per_second.iter().sum();
        self.combat_value_stars_total = self.combat_value_stars.iter().sum();
        self.combat_value_percent = normalized_parts(self.combat_values, self.combat_value_total);
        self.hitpoint_percent = normalized_parts(self.hitpoints, self.hitpoint_total);
        self.normalized_stars =
            normalize_to_largest(self.combat_value_stars, self.combat_value_stars_total);
    }

    /// Read a retail component. The original accessor omits raw HP/SP/DPS,
    /// despite declaring selectors for them, so those selectors return zero.
    #[must_use]
    pub fn component(&self, component: AISquadAnalysisComponent) -> f32 {
        use AISquadAnalysisComponent as C;
        match component {
            C::CVLight => self.combat_values[0],
            C::CVLightArmored => self.combat_values[1],
            C::CVMedium => self.combat_values[2],
            C::CVMediumAir => self.combat_values[3],
            C::CVHeavy => self.combat_values[4],
            C::CVBuilding => self.combat_values[5],
            C::CVTotal => self.combat_value_total,
            C::CVPercentLight => self.combat_value_percent[0],
            C::CVPercentLightArmored => self.combat_value_percent[1],
            C::CVPercentMedium => self.combat_value_percent[2],
            C::CVPercentMediumAir => self.combat_value_percent[3],
            C::CVPercentHeavy => self.combat_value_percent[4],
            C::CVPercentBuilding => self.combat_value_percent[5],
            C::HPPercentLight => self.hitpoint_percent[0],
            C::HPPercentLightArmored => self.hitpoint_percent[1],
            C::HPPercentMedium => self.hitpoint_percent[2],
            C::HPPercentMediumAir => self.hitpoint_percent[3],
            C::HPPercentHeavy => self.hitpoint_percent[4],
            C::HPPercentBuilding => self.hitpoint_percent[5],
            C::CVStarsLight => self.combat_value_stars[0],
            C::CVStarsLightArmored => self.combat_value_stars[1],
            C::CVStarsMedium => self.combat_value_stars[2],
            C::CVStarsMediumAir => self.combat_value_stars[3],
            C::CVStarsHeavy => self.combat_value_stars[4],
            C::CVStarsBuilding => self.combat_value_stars[5],
            C::CVStarsTotal => self.combat_value_stars_total,
            C::NormalizedStarsLight => self.normalized_stars[0],
            C::NormalizedStarsLightArmored => self.normalized_stars[1],
            C::NormalizedStarsMedium => self.normalized_stars[2],
            C::NormalizedStarsMediumAir => self.normalized_stars[3],
            C::NormalizedStarsHeavy => self.normalized_stars[4],
            C::NormalizedStarsBuilding => self.normalized_stars[5],
            C::Invalid
            | C::HPLight
            | C::HPLightArmored
            | C::HPMedium
            | C::HPMediumAir
            | C::HPHeavy
            | C::HPBuilding
            | C::HPTotal
            | C::SPLight
            | C::SPLightArmored
            | C::SPMedium
            | C::SPMediumAir
            | C::SPHeavy
            | C::SPBuilding
            | C::SPTotal
            | C::DPSLight
            | C::DPSLightArmored
            | C::DPSMedium
            | C::DPSMediumAir
            | C::DPSHeavy
            | C::DPSBuilding
            | C::DPSTotal => 0.0,
        }
    }

    /// Retail weighted offense of this analysis against another force.
    #[must_use]
    pub fn offense_against(&self, target: &Self) -> f32 {
        self.combat_value_stars
            .iter()
            .zip(target.combat_value_percent)
            .map(|(stars, percent)| stars * percent)
            .sum()
    }

    /// Retail A-to-B share of the two forces' combined offense.
    #[must_use]
    pub fn offense_ratio_against(&self, target: &Self) -> f32 {
        let offense_a = self.offense_against(target);
        let offense_b = target.offense_against(self);
        let total = offense_a + offense_b;
        if total > 0.0 {
            offense_a / total
        } else if self.can_hurt(target) && !target.can_hurt(self) {
            1.0
        } else {
            0.0
        }
    }

    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        for values in [
            self.combat_values,
            self.hitpoints,
            self.shieldpoints,
            self.damage_per_second,
            self.combat_value_percent,
            self.hitpoint_percent,
            self.combat_value_stars,
            self.normalized_stars,
        ] {
            for value in values {
                checksum.hash_f32(value);
            }
        }
        for total in [
            self.combat_value_total,
            self.hitpoint_total,
            self.shieldpoint_total,
            self.damage_per_second_total,
            self.combat_value_stars_total,
        ] {
            checksum.hash_f32(total);
        }
    }

    fn can_hurt(&self, target: &Self) -> bool {
        self.normalized_stars
            .iter()
            .zip(target.hitpoint_percent)
            .any(|(stars, percent)| stars * percent > 0.0)
    }
}

fn normalized_parts(
    values: [f32; AI_DAMAGE_CLASS_COUNT],
    total: f32,
) -> [f32; AI_DAMAGE_CLASS_COUNT] {
    if total > 0.0 {
        values.map(|value| value / total)
    } else {
        [0.0; AI_DAMAGE_CLASS_COUNT]
    }
}

fn normalize_to_largest(
    values: [f32; AI_DAMAGE_CLASS_COUNT],
    total: f32,
) -> [f32; AI_DAMAGE_CLASS_COUNT] {
    if total <= 0.0 {
        return [0.0; AI_DAMAGE_CLASS_COUNT];
    }
    let largest = values.into_iter().fold(0.0_f32, f32::max);
    if largest > 0.0 {
        values.map(|value| value / largest)
    } else {
        [0.0; AI_DAMAGE_CLASS_COUNT]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn component_ids_and_names_match_retail_order() {
        assert_eq!(AISquadAnalysisComponent::CVLight as u8, 1);
        assert_eq!(AISquadAnalysisComponent::CVStarsTotal as u8, 47);
        assert_eq!(AISquadAnalysisComponent::NormalizedStarsBuilding as u8, 53);
        assert_eq!(
            AISquadAnalysisComponent::from_name("CVPercentMediumAir"),
            Some(AISquadAnalysisComponent::CVPercentMediumAir)
        );
    }

    #[test]
    fn analysis_components_and_offense_follow_retail_formulas() {
        let mut light = AISquadAnalysis::default();
        light.add_squad(
            Some(AIDamageClass::Light),
            100.0,
            50.0,
            0.0,
            10.0,
            [10.0, 0.0, 20.0, 0.0, 0.0, 0.0],
        );
        light.finish();
        let mut medium = AISquadAnalysis::default();
        medium.add_squad(
            Some(AIDamageClass::Medium),
            80.0,
            80.0,
            0.0,
            10.0,
            [5.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        );
        medium.finish();

        assert!((light.component(AISquadAnalysisComponent::CVTotal) - 100.0).abs() < f32::EPSILON);
        assert!(
            (light.component(AISquadAnalysisComponent::CVPercentLight) - 1.0).abs() < f32::EPSILON
        );
        assert!(light.component(AISquadAnalysisComponent::HPTotal).abs() < f32::EPSILON);
        assert!((light.offense_against(&medium) - 200.0).abs() < f32::EPSILON);
        assert!((medium.offense_against(&light) - 40.0).abs() < f32::EPSILON);
        assert!((light.offense_ratio_against(&medium) - (5.0 / 6.0)).abs() < 0.000_1);
    }
}
