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
    /// No component. This is the default.
    #[default]
    Invalid = 0,
    /// Combat value of squads in the `Light` armor class.
    CVLight = 1,
    /// Combat value of squads in the `LightArmored` armor class.
    CVLightArmored = 2,
    /// Combat value of squads in the `Medium` armor class.
    CVMedium = 3,
    /// Combat value of squads in the `MediumAir` armor class.
    CVMediumAir = 4,
    /// Combat value of squads in the `Heavy` armor class.
    CVHeavy = 5,
    /// Combat value of squads in the `Building` armor class.
    CVBuilding = 6,
    /// Total combat value of all armor classes.
    CVTotal = 7,
    /// Hitpoints of squads in the `Light` armor class.
    HPLight = 8,
    /// Hitpoints of squads in the `LightArmored` armor class.
    HPLightArmored = 9,
    /// Hitpoints of squads in the `Medium` armor class.
    HPMedium = 10,
    /// Hitpoints of squads in the `MediumAir` armor class.
    HPMediumAir = 11,
    /// Hitpoints of squads in the `Heavy` armor class.
    HPHeavy = 12,
    /// Hitpoints of squads in the `Building` armor class.
    HPBuilding = 13,
    /// Total hitpoints of all armor classes.
    HPTotal = 14,
    /// Shieldpoints of squads in the `Light` armor class.
    SPLight = 15,
    /// Shieldpoints of squads in the `LightArmored` armor class.
    SPLightArmored = 16,
    /// Shieldpoints of squads in the `Medium` armor class.
    SPMedium = 17,
    /// Shieldpoints of squads in the `MediumAir` armor class.
    SPMediumAir = 18,
    /// Shieldpoints of squads in the `Heavy` armor class.
    SPHeavy = 19,
    /// Shieldpoints of squads in the `Building` armor class.
    SPBuilding = 20,
    /// Total shieldpoints of all armor classes.
    SPTotal = 21,
    /// Damage per second against the `Light` armor class.
    DPSLight = 22,
    /// Damage per second against the `LightArmored` armor class.
    DPSLightArmored = 23,
    /// Damage per second against the `Medium` armor class.
    DPSMedium = 24,
    /// Damage per second against the `MediumAir` armor class.
    DPSMediumAir = 25,
    /// Damage per second against the `Heavy` armor class.
    DPSHeavy = 26,
    /// Damage per second against the `Building` armor class.
    DPSBuilding = 27,
    /// Total damage per second against all armor classes.
    DPSTotal = 28,
    /// Share of combat value in the `Light` armor class, from 0 to 1.
    CVPercentLight = 29,
    /// Share of combat value in the `LightArmored` armor class, from 0 to 1.
    CVPercentLightArmored = 30,
    /// Share of combat value in the `Medium` armor class, from 0 to 1.
    CVPercentMedium = 31,
    /// Share of combat value in the `MediumAir` armor class, from 0 to 1.
    CVPercentMediumAir = 32,
    /// Share of combat value in the `Heavy` armor class, from 0 to 1.
    CVPercentHeavy = 33,
    /// Share of combat value in the `Building` armor class, from 0 to 1.
    CVPercentBuilding = 34,
    /// Share of hitpoints in the `Light` armor class, from 0 to 1.
    HPPercentLight = 35,
    /// Share of hitpoints in the `LightArmored` armor class, from 0 to 1.
    HPPercentLightArmored = 36,
    /// Share of hitpoints in the `Medium` armor class, from 0 to 1.
    HPPercentMedium = 37,
    /// Share of hitpoints in the `MediumAir` armor class, from 0 to 1.
    HPPercentMediumAir = 38,
    /// Share of hitpoints in the `Heavy` armor class, from 0 to 1.
    HPPercentHeavy = 39,
    /// Share of hitpoints in the `Building` armor class, from 0 to 1.
    HPPercentBuilding = 40,
    /// Combat value times attack stars against the `Light` armor class.
    CVStarsLight = 41,
    /// Combat value times attack stars against the `LightArmored` armor class.
    CVStarsLightArmored = 42,
    /// Combat value times attack stars against the `Medium` armor class.
    CVStarsMedium = 43,
    /// Combat value times attack stars against the `MediumAir` armor class.
    CVStarsMediumAir = 44,
    /// Combat value times attack stars against the `Heavy` armor class.
    CVStarsHeavy = 45,
    /// Combat value times attack stars against the `Building` armor class.
    CVStarsBuilding = 46,
    /// Total combat-value stars of all armor classes.
    CVStarsTotal = 47,
    /// Combat-value stars for the `Light` armor class, scaled so the largest class is 1.
    NormalizedStarsLight = 48,
    /// Combat-value stars for the `LightArmored` armor class, scaled so the largest class is 1.
    NormalizedStarsLightArmored = 49,
    /// Combat-value stars for the `Medium` armor class, scaled so the largest class is 1.
    NormalizedStarsMedium = 50,
    /// Combat-value stars for the `MediumAir` armor class, scaled so the largest class is 1.
    NormalizedStarsMediumAir = 51,
    /// Combat-value stars for the `Heavy` armor class, scaled so the largest class is 1.
    NormalizedStarsHeavy = 52,
    /// Combat-value stars for the `Building` armor class, scaled so the largest class is 1.
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
