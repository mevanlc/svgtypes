// Copyright 2024 the SVG Types Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! CSS `color-mix()` function support.
//!
//! This module implements parsing and evaluation of the CSS Color Level 5
//! `color-mix()` function. Only CSS Color Level 3 inputs are supported.

use crate::Color;

#[cfg(feature = "color-mix")]
use palette::{Hsl, IntoColor, Lab, Lch, LinSrgb, Oklab, Oklch, Srgb};

/// Color space for color interpolation in `color-mix()`.
///
/// See <https://www.w3.org/TR/css-color-5/#color-mix>
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ColorSpace {
    /// sRGB color space (gamma-encoded).
    Srgb,
    /// Linear sRGB color space.
    SrgbLinear,
    /// CIE Lab color space.
    Lab,
    /// Oklab color space (default per CSS spec).
    #[default]
    Oklab,
    /// CIE LCH color space (polar form of Lab).
    Lch,
    /// OkLCH color space (polar form of Oklab).
    Oklch,
    /// HSL color space.
    Hsl,
    /// HWB color space.
    Hwb,
}

impl ColorSpace {
    /// Returns true if this is a polar/cylindrical color space with a hue component.
    #[inline]
    pub fn is_polar(&self) -> bool {
        matches!(self, ColorSpace::Hsl | ColorSpace::Hwb | ColorSpace::Lch | ColorSpace::Oklch)
    }
}

/// Hue interpolation method for polar color spaces.
///
/// See <https://www.w3.org/TR/css-color-4/#hue-interpolation>
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum HueInterpolation {
    /// Take the shorter arc (default).
    #[default]
    Shorter,
    /// Take the longer arc.
    Longer,
    /// Interpolate with increasing hue.
    Increasing,
    /// Interpolate with decreasing hue.
    Decreasing,
}

/// Normalize percentages for color-mix.
///
/// Returns (p1, p2, alpha_multiplier) where p1 + p2 = 1.0 (after normalization).
fn normalize_percentages(p1: Option<f32>, p2: Option<f32>) -> (f32, f32, f32) {
    let (p1, p2) = match (p1, p2) {
        (None, None) => (0.5, 0.5),
        (Some(p), None) => (p, 1.0 - p),
        (None, Some(p)) => (1.0 - p, p),
        (Some(a), Some(b)) => (a, b),
    };

    let sum = p1 + p2;
    if sum <= 0.0 {
        return (0.0, 0.0, 0.0); // transparent
    }

    let alpha_mult = sum.min(1.0);
    (p1 / sum, p2 / sum, alpha_mult)
}

/// Interpolate hue values according to the specified method.
fn interpolate_hue(h1: f32, h2: f32, p1: f32, p2: f32, method: HueInterpolation) -> f32 {
    let mut h1 = h1 % 360.0;
    let mut h2 = h2 % 360.0;
    if h1 < 0.0 {
        h1 += 360.0;
    }
    if h2 < 0.0 {
        h2 += 360.0;
    }

    let diff = h2 - h1;

    match method {
        HueInterpolation::Shorter => {
            if diff > 180.0 {
                h1 += 360.0;
            } else if diff < -180.0 {
                h2 += 360.0;
            }
        }
        HueInterpolation::Longer => {
            if diff > 0.0 && diff < 180.0 {
                h1 += 360.0;
            } else if diff > -180.0 && diff < 0.0 {
                h2 += 360.0;
            }
        }
        HueInterpolation::Increasing => {
            if diff < 0.0 {
                h2 += 360.0;
            }
        }
        HueInterpolation::Decreasing => {
            if diff > 0.0 {
                h1 += 360.0;
            }
        }
    }

    let result = h1 * p1 + h2 * p2;
    ((result % 360.0) + 360.0) % 360.0
}

/// Compute the color-mix result.
#[cfg(feature = "color-mix")]
pub fn compute_color_mix(
    space: ColorSpace,
    hue_interp: HueInterpolation,
    c1: Color,
    p1: Option<f32>,
    c2: Color,
    p2: Option<f32>,
) -> Color {
    let (p1, p2, alpha_mult) = normalize_percentages(p1, p2);

    if alpha_mult == 0.0 {
        return Color::new_rgba(0, 0, 0, 0);
    }

    // Convert u8 to f32 normalized values
    let r1 = c1.red as f32 / 255.0;
    let g1 = c1.green as f32 / 255.0;
    let b1 = c1.blue as f32 / 255.0;
    let a1 = c1.alpha as f32 / 255.0;

    let r2 = c2.red as f32 / 255.0;
    let g2 = c2.green as f32 / 255.0;
    let b2 = c2.blue as f32 / 255.0;
    let a2 = c2.alpha as f32 / 255.0;

    // Premultiply alpha
    let (pr1, pg1, pb1) = (r1 * a1, g1 * a1, b1 * a1);
    let (pr2, pg2, pb2) = (r2 * a2, g2 * a2, b2 * a2);

    // Mix in the specified color space
    let (mixed_r, mixed_g, mixed_b) = match space {
        ColorSpace::Srgb => mix_in_srgb(pr1, pg1, pb1, pr2, pg2, pb2, p1, p2),
        ColorSpace::SrgbLinear => mix_in_srgb_linear(pr1, pg1, pb1, pr2, pg2, pb2, p1, p2),
        ColorSpace::Oklab => mix_in_oklab(pr1, pg1, pb1, pr2, pg2, pb2, p1, p2),
        ColorSpace::Lab => mix_in_lab(pr1, pg1, pb1, pr2, pg2, pb2, p1, p2),
        ColorSpace::Hsl => mix_in_hsl(pr1, pg1, pb1, pr2, pg2, pb2, p1, p2, hue_interp),
        ColorSpace::Lch => mix_in_lch(pr1, pg1, pb1, pr2, pg2, pb2, p1, p2, hue_interp),
        ColorSpace::Oklch => mix_in_oklch(pr1, pg1, pb1, pr2, pg2, pb2, p1, p2, hue_interp),
        ColorSpace::Hwb => mix_in_hwb(pr1, pg1, pb1, pr2, pg2, pb2, p1, p2, hue_interp),
    };

    // Mix alpha
    let mixed_a = a1 * p1 + a2 * p2;

    // Un-premultiply alpha
    let (final_r, final_g, final_b) = if mixed_a > 0.0 {
        (mixed_r / mixed_a, mixed_g / mixed_a, mixed_b / mixed_a)
    } else {
        (0.0, 0.0, 0.0)
    };

    // Apply alpha multiplier (for when percentages sum < 100%)
    let final_a = mixed_a * alpha_mult;

    // Convert back to u8
    Color::new_rgba(
        (final_r.clamp(0.0, 1.0) * 255.0).round() as u8,
        (final_g.clamp(0.0, 1.0) * 255.0).round() as u8,
        (final_b.clamp(0.0, 1.0) * 255.0).round() as u8,
        (final_a.clamp(0.0, 1.0) * 255.0).round() as u8,
    )
}

#[cfg(feature = "color-mix")]
fn mix_in_srgb(
    r1: f32, g1: f32, b1: f32,
    r2: f32, g2: f32, b2: f32,
    p1: f32, p2: f32,
) -> (f32, f32, f32) {
    (
        r1 * p1 + r2 * p2,
        g1 * p1 + g2 * p2,
        b1 * p1 + b2 * p2,
    )
}

#[cfg(feature = "color-mix")]
fn mix_in_srgb_linear(
    r1: f32, g1: f32, b1: f32,
    r2: f32, g2: f32, b2: f32,
    p1: f32, p2: f32,
) -> (f32, f32, f32) {
    let srgb1 = Srgb::new(r1, g1, b1);
    let srgb2 = Srgb::new(r2, g2, b2);

    let lin1: LinSrgb = srgb1.into_color();
    let lin2: LinSrgb = srgb2.into_color();

    let mixed = LinSrgb::new(
        lin1.red * p1 + lin2.red * p2,
        lin1.green * p1 + lin2.green * p2,
        lin1.blue * p1 + lin2.blue * p2,
    );

    let result: Srgb = mixed.into_color();
    (result.red, result.green, result.blue)
}

#[cfg(feature = "color-mix")]
fn mix_in_oklab(
    r1: f32, g1: f32, b1: f32,
    r2: f32, g2: f32, b2: f32,
    p1: f32, p2: f32,
) -> (f32, f32, f32) {
    let srgb1 = Srgb::new(r1, g1, b1);
    let srgb2 = Srgb::new(r2, g2, b2);

    let oklab1: Oklab = srgb1.into_color();
    let oklab2: Oklab = srgb2.into_color();

    let mixed = Oklab::new(
        oklab1.l * p1 + oklab2.l * p2,
        oklab1.a * p1 + oklab2.a * p2,
        oklab1.b * p1 + oklab2.b * p2,
    );

    let result: Srgb = mixed.into_color();
    (
        result.red.clamp(0.0, 1.0),
        result.green.clamp(0.0, 1.0),
        result.blue.clamp(0.0, 1.0),
    )
}

#[cfg(feature = "color-mix")]
fn mix_in_lab(
    r1: f32, g1: f32, b1: f32,
    r2: f32, g2: f32, b2: f32,
    p1: f32, p2: f32,
) -> (f32, f32, f32) {
    let srgb1 = Srgb::new(r1, g1, b1);
    let srgb2 = Srgb::new(r2, g2, b2);

    let lab1: Lab = srgb1.into_color();
    let lab2: Lab = srgb2.into_color();

    let mixed = Lab::new(
        lab1.l * p1 + lab2.l * p2,
        lab1.a * p1 + lab2.a * p2,
        lab1.b * p1 + lab2.b * p2,
    );

    let result: Srgb = mixed.into_color();
    (
        result.red.clamp(0.0, 1.0),
        result.green.clamp(0.0, 1.0),
        result.blue.clamp(0.0, 1.0),
    )
}

#[cfg(feature = "color-mix")]
fn mix_in_hsl(
    r1: f32, g1: f32, b1: f32,
    r2: f32, g2: f32, b2: f32,
    p1: f32, p2: f32,
    hue_interp: HueInterpolation,
) -> (f32, f32, f32) {
    let srgb1 = Srgb::new(r1, g1, b1);
    let srgb2 = Srgb::new(r2, g2, b2);

    let hsl1: Hsl = srgb1.into_color();
    let hsl2: Hsl = srgb2.into_color();

    let mixed_hue = interpolate_hue(
        hsl1.hue.into_positive_degrees(),
        hsl2.hue.into_positive_degrees(),
        p1, p2, hue_interp,
    );

    let mixed = Hsl::new(
        mixed_hue,
        hsl1.saturation * p1 + hsl2.saturation * p2,
        hsl1.lightness * p1 + hsl2.lightness * p2,
    );

    let result: Srgb = mixed.into_color();
    (
        result.red.clamp(0.0, 1.0),
        result.green.clamp(0.0, 1.0),
        result.blue.clamp(0.0, 1.0),
    )
}

#[cfg(feature = "color-mix")]
fn mix_in_lch(
    r1: f32, g1: f32, b1: f32,
    r2: f32, g2: f32, b2: f32,
    p1: f32, p2: f32,
    hue_interp: HueInterpolation,
) -> (f32, f32, f32) {
    let srgb1 = Srgb::new(r1, g1, b1);
    let srgb2 = Srgb::new(r2, g2, b2);

    let lch1: Lch = srgb1.into_color();
    let lch2: Lch = srgb2.into_color();

    let mixed_hue = interpolate_hue(
        lch1.hue.into_positive_degrees(),
        lch2.hue.into_positive_degrees(),
        p1, p2, hue_interp,
    );

    let mixed = Lch::new(
        lch1.l * p1 + lch2.l * p2,
        lch1.chroma * p1 + lch2.chroma * p2,
        mixed_hue,
    );

    let result: Srgb = mixed.into_color();
    (
        result.red.clamp(0.0, 1.0),
        result.green.clamp(0.0, 1.0),
        result.blue.clamp(0.0, 1.0),
    )
}

#[cfg(feature = "color-mix")]
fn mix_in_oklch(
    r1: f32, g1: f32, b1: f32,
    r2: f32, g2: f32, b2: f32,
    p1: f32, p2: f32,
    hue_interp: HueInterpolation,
) -> (f32, f32, f32) {
    let srgb1 = Srgb::new(r1, g1, b1);
    let srgb2 = Srgb::new(r2, g2, b2);

    let oklch1: Oklch = srgb1.into_color();
    let oklch2: Oklch = srgb2.into_color();

    let mixed_hue = interpolate_hue(
        oklch1.hue.into_positive_degrees(),
        oklch2.hue.into_positive_degrees(),
        p1, p2, hue_interp,
    );

    let mixed = Oklch::new(
        oklch1.l * p1 + oklch2.l * p2,
        oklch1.chroma * p1 + oklch2.chroma * p2,
        mixed_hue,
    );

    let result: Srgb = mixed.into_color();
    (
        result.red.clamp(0.0, 1.0),
        result.green.clamp(0.0, 1.0),
        result.blue.clamp(0.0, 1.0),
    )
}

#[cfg(feature = "color-mix")]
fn mix_in_hwb(
    r1: f32, g1: f32, b1: f32,
    r2: f32, g2: f32, b2: f32,
    p1: f32, p2: f32,
    hue_interp: HueInterpolation,
) -> (f32, f32, f32) {
    use palette::Hwb;

    let srgb1 = Srgb::new(r1, g1, b1);
    let srgb2 = Srgb::new(r2, g2, b2);

    let hwb1: Hwb = srgb1.into_color();
    let hwb2: Hwb = srgb2.into_color();

    let mixed_hue = interpolate_hue(
        hwb1.hue.into_positive_degrees(),
        hwb2.hue.into_positive_degrees(),
        p1, p2, hue_interp,
    );

    let mixed = Hwb::new(
        mixed_hue,
        hwb1.whiteness * p1 + hwb2.whiteness * p2,
        hwb1.blackness * p1 + hwb2.blackness * p2,
    );

    let result: Srgb = mixed.into_color();
    (
        result.red.clamp(0.0, 1.0),
        result.green.clamp(0.0, 1.0),
        result.blue.clamp(0.0, 1.0),
    )
}

/// Stub implementation when color-mix feature is disabled.
#[cfg(not(feature = "color-mix"))]
pub fn compute_color_mix(
    _space: ColorSpace,
    _hue_interp: HueInterpolation,
    _c1: Color,
    _p1: Option<f32>,
    _c2: Color,
    _p2: Option<f32>,
) -> Color {
    // Return black when feature is disabled
    Color::black()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_percentages() {
        // Both omitted -> 50/50
        let (p1, p2, mult) = normalize_percentages(None, None);
        assert!((p1 - 0.5).abs() < 0.001);
        assert!((p2 - 0.5).abs() < 0.001);
        assert!((mult - 1.0).abs() < 0.001);

        // One specified
        let (p1, p2, mult) = normalize_percentages(Some(0.3), None);
        assert!((p1 - 0.3).abs() < 0.001);
        assert!((p2 - 0.7).abs() < 0.001);
        assert!((mult - 1.0).abs() < 0.001);

        // Both specified, sum = 1
        let (p1, p2, mult) = normalize_percentages(Some(0.25), Some(0.75));
        assert!((p1 - 0.25).abs() < 0.001);
        assert!((p2 - 0.75).abs() < 0.001);
        assert!((mult - 1.0).abs() < 0.001);

        // Both specified, sum < 1 (should reduce alpha)
        let (p1, p2, mult) = normalize_percentages(Some(0.2), Some(0.3));
        assert!((p1 - 0.4).abs() < 0.001); // normalized: 0.2 / 0.5
        assert!((p2 - 0.6).abs() < 0.001); // normalized: 0.3 / 0.5
        assert!((mult - 0.5).abs() < 0.001);

        // Both zero -> transparent
        let (p1, p2, mult) = normalize_percentages(Some(0.0), Some(0.0));
        assert!((mult - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_interpolate_hue_shorter() {
        // Shorter path: 350 -> 10 should go through 0
        let h = interpolate_hue(350.0, 10.0, 0.5, 0.5, HueInterpolation::Shorter);
        // Expected: (350 + 370) / 2 = 360, normalized to 0
        assert!(h < 10.0 || h > 350.0);

        // Simple case: 0 -> 60
        let h = interpolate_hue(0.0, 60.0, 0.5, 0.5, HueInterpolation::Shorter);
        assert!((h - 30.0).abs() < 0.001);
    }

    #[test]
    fn test_interpolate_hue_longer() {
        // Longer path: 0 -> 60 should go the long way (through 180, 270...)
        let h = interpolate_hue(0.0, 60.0, 0.5, 0.5, HueInterpolation::Longer);
        // Expected: (360 + 60) / 2 = 210
        assert!((h - 210.0).abs() < 0.001);
    }

    #[test]
    fn test_color_space_is_polar() {
        assert!(!ColorSpace::Srgb.is_polar());
        assert!(!ColorSpace::Oklab.is_polar());
        assert!(ColorSpace::Hsl.is_polar());
        assert!(ColorSpace::Lch.is_polar());
        assert!(ColorSpace::Oklch.is_polar());
        assert!(ColorSpace::Hwb.is_polar());
    }
}
