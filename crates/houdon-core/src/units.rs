//! Units for showing sizes to the artist. Everything inside is metres.

/// How sizes, heights and distances are shown to the artist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum UnitSystem {
    /// Centimetres and metres.
    #[default]
    Metric,
    /// Inches and feet.
    Imperial,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LengthUnit {
    Centimetre,
    Metre,
    Inch,
    Foot,
}

impl LengthUnit {
    /// Length of one unit in metres.
    pub fn metres(self) -> f32 {
        match self {
            Self::Centimetre => 0.01,
            Self::Metre => 1.0,
            Self::Inch => 0.0254,
            Self::Foot => 0.3048,
        }
    }
}

/// A length expressed in a unit for display.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Length {
    pub value: f32,
    pub unit: LengthUnit,
}

impl UnitSystem {
    pub const ALL: [Self; 2] = [Self::Metric, Self::Imperial];

    /// A length in the most readable unit: centimetres below a metre or
    /// inches below a foot.
    pub fn readable(self, metres: f32) -> Length {
        let (small, large) = match self {
            Self::Metric => (LengthUnit::Centimetre, LengthUnit::Metre),
            Self::Imperial => (LengthUnit::Inch, LengthUnit::Foot),
        };
        let unit = if metres.abs() < large.metres() {
            small
        } else {
            large
        };
        Length {
            value: metres / unit.metres(),
            unit,
        }
    }

    /// The unit used in number fields: metres or feet.
    pub fn editing_unit(self) -> LengthUnit {
        match self {
            Self::Metric => LengthUnit::Metre,
            Self::Imperial => LengthUnit::Foot,
        }
    }

    /// Metres to the value shown in a number field.
    pub fn to_editing(self, metres: f32) -> f32 {
        metres / self.editing_unit().metres()
    }

    /// The value typed in a number field back to metres.
    pub fn from_editing(self, value: f32) -> f32 {
        value * self.editing_unit().metres()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn metric_lengths_use_centimetres_below_a_metre() {
        let l = UnitSystem::Metric.readable(1.75);
        assert_eq!(l.unit, LengthUnit::Metre);
        assert!(close(l.value, 1.75));
        let l = UnitSystem::Metric.readable(0.2);
        assert_eq!(l.unit, LengthUnit::Centimetre);
        assert!(close(l.value, 20.0));
    }

    #[test]
    fn imperial_lengths_use_inches_below_a_foot() {
        let l = UnitSystem::Imperial.readable(1.8288);
        assert_eq!(l.unit, LengthUnit::Foot);
        assert!(close(l.value, 6.0));
        let l = UnitSystem::Imperial.readable(0.0254);
        assert_eq!(l.unit, LengthUnit::Inch);
        assert!(close(l.value, 1.0));
    }

    #[test]
    fn editing_units_round_trip() {
        for system in [UnitSystem::Metric, UnitSystem::Imperial] {
            let shown = system.to_editing(1.75);
            assert!(close(system.from_editing(shown), 1.75));
        }
        assert_eq!(UnitSystem::Metric.editing_unit(), LengthUnit::Metre);
        assert_eq!(UnitSystem::Imperial.editing_unit(), LengthUnit::Foot);
        assert!(close(UnitSystem::Imperial.to_editing(0.3048), 1.0));
    }
}
