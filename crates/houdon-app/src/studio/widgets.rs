//! Small controls and texts shared by the Studio's sections.

use super::*;

/// Message id of a Solid light's name.
pub(super) fn solid_lighting_message_id(lighting: SolidLighting) -> &'static str {
    match lighting {
        SolidLighting::Studio => "solid-studio",
        SolidLighting::Matcap => "solid-matcap",
        SolidLighting::Flat => "solid-flat",
    }
}

/// Message id of a colour source's name.
pub(super) fn color_source_message_id(source: ColorSource) -> &'static str {
    match source {
        ColorSource::Material => "color-material",
        ColorSource::Clay => "color-clay",
        ColorSource::Random => "color-random",
    }
}

/// Message id of a built-in room's name.
pub(super) fn builtin_room_message_id(room: BuiltinRoom) -> &'static str {
    match room {
        BuiltinRoom::Grey => "room-grey",
        BuiltinRoom::White => "room-white",
        BuiltinRoom::Dark => "room-dark",
    }
}

/// Message id of a light kind's name.
pub(super) fn light_kind_message_id(kind: LightKind) -> &'static str {
    match kind {
        LightKind::Directional => "light-directional",
        LightKind::Point => "light-point",
        LightKind::Spot => "light-spot",
    }
}

/// Message id of a unit's short name.
pub fn unit_message_id(unit: LengthUnit) -> &'static str {
    match unit {
        LengthUnit::Centimetre => "unit-centimetre-short",
        LengthUnit::Metre => "unit-metre-short",
        LengthUnit::Inch => "unit-inch-short",
        LengthUnit::Foot => "unit-foot-short",
    }
}

/// A length in the most readable unit of `units`, such as "1.75 m" or "20 cm".
pub(super) fn format_length(tr: &Localizer, units: UnitSystem, metres: f32) -> String {
    let length = units.readable(metres);
    let decimals = match length.unit {
        LengthUnit::Metre | LengthUnit::Foot => 2,
        LengthUnit::Centimetre | LengthUnit::Inch => 1,
    };
    tr.tr_args(
        "length",
        &[
            ("value", format!("{:.*}", decimals, length.value).into()),
            ("unit", tr.tr(unit_message_id(length.unit)).into()),
        ],
    )
}

/// A number field for a length kept in metres, shown and typed in the
/// editing unit of `units` (metres or feet). `range` is in metres.
/// Returns whether the artist changed it.
pub fn length_field(
    ui: &mut egui::Ui,
    tr: &Localizer,
    units: UnitSystem,
    metres: &mut f32,
    range: std::ops::RangeInclusive<f32>,
) -> bool {
    let suffix = format!(" {}", tr.tr(unit_message_id(units.editing_unit())));
    let mut shown = units.to_editing(*metres);
    let changed = ui
        .add(
            egui::DragValue::new(&mut shown)
                .speed(0.01)
                .range(units.to_editing(*range.start())..=units.to_editing(*range.end()))
                .suffix(suffix)
                .max_decimals(3),
        )
        .changed();
    if changed {
        *metres = units.from_editing(shown);
    }
    changed
}

/// An angle in degrees brought into -180..180.
pub(super) fn wrap_degrees(degrees: f32) -> f32 {
    (degrees + 180.0).rem_euclid(360.0) - 180.0
}

/// Snaps values that are zero up to rounding to exactly 0, so they do not
/// display as "-0".
pub(super) fn snap_to_zero(value: f32) -> f32 {
    if value.abs() < 1e-4 { 0.0 } else { value }
}
