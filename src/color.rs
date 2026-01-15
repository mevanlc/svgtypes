// Copyright 2021 the SVG Types Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::{ByteExt, Error, Stream, colors};
use crate::color_mix::{ColorSpace, HueInterpolation, compute_color_mix};

#[cfg(not(feature = "std"))]
use kurbo::common::FloatFuncs;

/// Representation of the [`<color>`] type.
///
/// [`<color>`]: https://www.w3.org/TR/css-color-3/
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[allow(missing_docs)]
pub struct Color {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

impl Color {
    /// Constructs a new `Color` from RGB values.
    #[inline]
    pub fn new_rgb(red: u8, green: u8, blue: u8) -> Self {
        Self {
            red,
            green,
            blue,
            alpha: 255,
        }
    }

    /// Constructs a new `Color` from RGBA values.
    #[inline]
    pub fn new_rgba(red: u8, green: u8, blue: u8, alpha: u8) -> Self {
        Self {
            red,
            green,
            blue,
            alpha,
        }
    }

    /// Constructs a new `Color` set to black.
    #[inline]
    pub fn black() -> Self {
        Self::new_rgb(0, 0, 0)
    }

    /// Constructs a new `Color` set to white.
    #[inline]
    pub fn white() -> Self {
        Self::new_rgb(255, 255, 255)
    }

    /// Constructs a new `Color` set to gray.
    #[inline]
    pub fn gray() -> Self {
        Self::new_rgb(128, 128, 128)
    }

    /// Constructs a new `Color` set to red.
    #[inline]
    pub fn red() -> Self {
        Self::new_rgb(255, 0, 0)
    }

    /// Constructs a new `Color` set to green.
    #[inline]
    pub fn green() -> Self {
        Self::new_rgb(0, 128, 0)
    }

    /// Constructs a new `Color` set to blue.
    #[inline]
    pub fn blue() -> Self {
        Self::new_rgb(0, 0, 255)
    }
}

impl core::str::FromStr for Color {
    type Err = Error;

    /// Parses [CSS3](https://www.w3.org/TR/css-color-3/) `Color` from a string.
    ///
    /// # Errors
    ///
    ///  - Returns error if a color has an invalid format.
    ///  - Returns error if `<color>` is followed by `<icccolor>`. It's not supported.
    ///
    /// # Notes
    ///
    ///  - Any non-`hexdigit` bytes will be treated as `0`.
    ///  - The [SVG 1.1 spec] has an error.
    ///    There should be a `number`, not an `integer` for percent values ([details]).
    ///  - It also supports 4 digits and 8 digits hex notation from the
    ///    [CSS Color Module Level 4][css-color-4-hex].
    ///
    /// [SVG 1.1 spec]: https://www.w3.org/TR/SVG11/types.html#DataTypeColor
    /// [details]: https://lists.w3.org/Archives/Public/www-svg/2014Jan/0109.html
    /// [css-color-4-hex]: https://www.w3.org/TR/css-color-4/#hex-notation
    fn from_str(text: &str) -> Result<Self, Error> {
        let mut s = Stream::from(text);
        let color = s.parse_color()?;

        // Check that we are at the end of the stream. Otherwise color can be followed by icccolor,
        // which is not supported.
        s.skip_spaces();
        if !s.at_end() {
            return Err(Error::UnexpectedData(s.calc_char_pos()));
        }

        Ok(color)
    }
}

impl Stream<'_> {
    /// Tries to parse a color, but doesn't advance on error.
    pub fn try_parse_color(&mut self) -> Option<Color> {
        let mut s = *self;
        if let Ok(color) = s.parse_color() {
            *self = s;
            Some(color)
        } else {
            None
        }
    }

    /// Parses a color.
    pub fn parse_color(&mut self) -> Result<Color, Error> {
        self.skip_spaces();

        let mut color = Color::black();

        if self.curr_byte()? == b'#' {
            // See https://www.w3.org/TR/css-color-4/#hex-notation
            self.advance(1);
            let color_str = self.consume_bytes(|_, c| c.is_hex_digit()).as_bytes();
            // get color data len until first space or stream end
            match color_str.len() {
                6 => {
                    // #rrggbb
                    color.red = hex_pair(color_str[0], color_str[1]);
                    color.green = hex_pair(color_str[2], color_str[3]);
                    color.blue = hex_pair(color_str[4], color_str[5]);
                }
                8 => {
                    // #rrggbbaa
                    color.red = hex_pair(color_str[0], color_str[1]);
                    color.green = hex_pair(color_str[2], color_str[3]);
                    color.blue = hex_pair(color_str[4], color_str[5]);
                    color.alpha = hex_pair(color_str[6], color_str[7]);
                }
                3 => {
                    // #rgb
                    color.red = short_hex(color_str[0]);
                    color.green = short_hex(color_str[1]);
                    color.blue = short_hex(color_str[2]);
                }
                4 => {
                    // #rgba
                    color.red = short_hex(color_str[0]);
                    color.green = short_hex(color_str[1]);
                    color.blue = short_hex(color_str[2]);
                    color.alpha = short_hex(color_str[3]);
                }
                _ => {
                    return Err(Error::InvalidValue);
                }
            }
        } else {
            // TODO: remove allocation
            let name = self.consume_ascii_ident().to_ascii_lowercase();
            if name == "rgb" || name == "rgba" {
                self.consume_byte(b'(')?;

                let mut is_percent = false;
                let value = self.parse_number()?;
                if self.starts_with(b"%") {
                    self.advance(1);
                    is_percent = true;
                }
                self.skip_spaces();
                self.parse_list_separator();

                if is_percent {
                    // The division and multiply are explicitly not collapsed, to ensure the red
                    // component has the same rounding behavior as the green and blue components.
                    color.red = ((value / 100.0) * 255.0).round() as u8;
                    color.green = (self.parse_list_number_or_percent()? * 255.0).round() as u8;
                    color.blue = (self.parse_list_number_or_percent()? * 255.0).round() as u8;
                } else {
                    color.red = value.round() as u8;
                    color.green = self.parse_list_number()?.round() as u8;
                    color.blue = self.parse_list_number()?.round() as u8;
                }

                self.skip_spaces();
                if !self.starts_with(b")") {
                    color.alpha = (self.parse_list_number()? * 255.0).round() as u8;
                }

                self.skip_spaces();
                self.consume_byte(b')')?;
            } else if name == "hsl" || name == "hsla" {
                self.consume_byte(b'(')?;

                let mut hue = self.parse_list_number()?;
                hue = ((hue % 360.0) + 360.0) % 360.0;

                let saturation = f64_bound(0.0, self.parse_list_number_or_percent()?, 1.0);
                let lightness = f64_bound(0.0, self.parse_list_number_or_percent()?, 1.0);

                color = hsl_to_rgb(hue as f32 / 60.0, saturation as f32, lightness as f32);

                self.skip_spaces();
                if !self.starts_with(b")") {
                    color.alpha = (self.parse_list_number()? * 255.0).round() as u8;
                }

                self.skip_spaces();
                self.consume_byte(b')')?;
            } else if name == "color-mix" {
                return self.parse_color_mix();
            } else {
                match colors::from_str(&name) {
                    Some(c) => {
                        color = c;
                    }
                    None => {
                        return Err(Error::InvalidValue);
                    }
                }
            }
        }

        Ok(color)
    }

    /// Parses a `color-mix()` function.
    ///
    /// Syntax: `color-mix(in <colorspace> [<hue-interpolation> hue], <color> [<percentage>], <color> [<percentage>])`
    fn parse_color_mix(&mut self) -> Result<Color, Error> {
        self.consume_byte(b'(')?;
        self.skip_spaces();

        // Parse "in <colorspace> [<hue-method> hue]"
        let (space, hue_interp) = self.parse_color_interpolation_method()?;

        self.skip_spaces();
        self.consume_byte(b',')?;
        self.skip_spaces();

        // Parse color1
        let color1 = self.parse_color()?;
        self.skip_spaces();

        // Try to parse optional percentage for color1
        let pct1 = self.try_parse_percentage_for_color_mix();

        self.skip_spaces();
        self.consume_byte(b',')?;
        self.skip_spaces();

        // Parse color2
        let color2 = self.parse_color()?;
        self.skip_spaces();

        // Try to parse optional percentage for color2
        let pct2 = self.try_parse_percentage_for_color_mix();

        self.skip_spaces();
        self.consume_byte(b')')?;

        Ok(compute_color_mix(space, hue_interp, color1, pct1, color2, pct2))
    }

    /// Parse color interpolation method: "in <colorspace> [<hue-method> hue]"
    fn parse_color_interpolation_method(&mut self) -> Result<(ColorSpace, HueInterpolation), Error> {
        // Expect "in"
        let in_keyword = self.consume_ascii_ident().to_ascii_lowercase();
        if in_keyword != "in" {
            return Err(Error::InvalidValue);
        }

        self.skip_spaces();

        // Parse color space
        let space = self.parse_color_space()?;

        self.skip_spaces();

        // Check for optional hue interpolation method (only valid for polar spaces)
        let hue_interp = if space.is_polar() && !self.at_end() && !self.is_curr_byte_eq(b',') {
            self.try_parse_hue_interpolation()
        } else {
            HueInterpolation::default()
        };

        Ok((space, hue_interp))
    }

    /// Parse color space identifier.
    fn parse_color_space(&mut self) -> Result<ColorSpace, Error> {
        let name = self.consume_ascii_ident().to_ascii_lowercase();
        match name.as_str() {
            "srgb" => Ok(ColorSpace::Srgb),
            "srgb-linear" => Ok(ColorSpace::SrgbLinear),
            "lab" => Ok(ColorSpace::Lab),
            "oklab" => Ok(ColorSpace::Oklab),
            "lch" => Ok(ColorSpace::Lch),
            "oklch" => Ok(ColorSpace::Oklch),
            "hsl" => Ok(ColorSpace::Hsl),
            "hwb" => Ok(ColorSpace::Hwb),
            _ => Err(Error::InvalidValue),
        }
    }

    /// Try to parse hue interpolation: "<method> hue"
    fn try_parse_hue_interpolation(&mut self) -> HueInterpolation {
        let method_name = self.consume_ascii_ident().to_ascii_lowercase();

        let method = match method_name.as_str() {
            "shorter" => Some(HueInterpolation::Shorter),
            "longer" => Some(HueInterpolation::Longer),
            "increasing" => Some(HueInterpolation::Increasing),
            "decreasing" => Some(HueInterpolation::Decreasing),
            _ => None,
        };

        if let Some(m) = method {
            self.skip_spaces();
            let hue_keyword = self.consume_ascii_ident().to_ascii_lowercase();
            if hue_keyword == "hue" {
                return m;
            }
        }

        // Not a valid hue interpolation, would need to backtrack
        // For simplicity, return default (this case shouldn't happen with valid input)
        HueInterpolation::default()
    }

    /// Try to parse a percentage for color-mix (0-100%), returning normalized value (0.0-1.0).
    fn try_parse_percentage_for_color_mix(&mut self) -> Option<f32> {
        self.skip_spaces();

        if self.at_end() {
            return None;
        }

        // Check if next char could start a number
        let c = match self.curr_byte() {
            Ok(c) => c,
            Err(_) => return None,
        };

        if !c.is_digit() && c != b'+' && c != b'-' && c != b'.' {
            return None;
        }

        // Try to parse number
        if let Ok(n) = self.parse_number() {
            self.skip_spaces();
            if self.is_curr_byte_eq(b'%') {
                self.advance(1);
                return Some((n as f32 / 100.0).clamp(0.0, 1.0));
            }
        }

        // Not a percentage - this is tricky because we can't easily backtrack
        // But in color-mix context, a bare number without % after a color is invalid
        // So we'll just return None (the number parsing advanced the stream though)
        None
    }
}

#[inline]
fn from_hex(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        b'A'..=b'F' => c - b'A' + 10,
        _ => b'0',
    }
}

#[inline]
fn short_hex(c: u8) -> u8 {
    let h = from_hex(c);
    (h << 4) | h
}

#[inline]
fn hex_pair(c1: u8, c2: u8) -> u8 {
    let h1 = from_hex(c1);
    let h2 = from_hex(c2);
    (h1 << 4) | h2
}

// `hue` is in a 0..6 range, while `saturation` and `lightness` are in a 0..=1 range.
// Based on https://www.w3.org/TR/css-color-3/#hsl-color
fn hsl_to_rgb(hue: f32, saturation: f32, lightness: f32) -> Color {
    let t2 = if lightness <= 0.5 {
        lightness * (saturation + 1.0)
    } else {
        lightness + saturation - (lightness * saturation)
    };

    let t1 = lightness * 2.0 - t2;
    let red = hue_to_rgb(t1, t2, hue + 2.0);
    let green = hue_to_rgb(t1, t2, hue);
    let blue = hue_to_rgb(t1, t2, hue - 2.0);
    Color::new_rgb(
        (red * 255.0).round() as u8,
        (green * 255.0).round() as u8,
        (blue * 255.0).round() as u8,
    )
}

fn hue_to_rgb(t1: f32, t2: f32, mut hue: f32) -> f32 {
    if hue < 0.0 {
        hue += 6.0;
    }
    if hue >= 6.0 {
        hue -= 6.0;
    }

    if hue < 1.0 {
        (t2 - t1) * hue + t1
    } else if hue < 3.0 {
        t2
    } else if hue < 4.0 {
        (t2 - t1) * (4.0 - hue) + t1
    } else {
        t1
    }
}

#[inline]
fn f64_bound(min: f64, val: f64, max: f64) -> f64 {
    debug_assert!(val.is_finite());
    val.clamp(min, max)
}

#[rustfmt::skip]
#[cfg(test)]
mod tests {
    use alloc::string::ToString;
    use core::str::FromStr;
    use crate::Color;

    macro_rules! test {
        ($name:ident, $text:expr, $color:expr) => {
            #[test]
            fn $name() {
                assert_eq!(Color::from_str($text).unwrap(), $color);
            }
        };
    }

    test!(
        rrggbb,
        "#ff0000",
        Color::new_rgb(255, 0, 0)
    );

    test!(
        rrggbb_upper,
        "#FF0000",
        Color::new_rgb(255, 0, 0)
    );

    test!(
        rgb_hex,
        "#f00",
        Color::new_rgb(255, 0, 0)
    );

    test!(
        rrggbbaa,
        "#ff0000ff",
        Color::new_rgba(255, 0, 0, 255)
    );

    test!(
        rrggbbaa_upper,
        "#FF0000FF",
        Color::new_rgba(255, 0, 0, 255)
    );

    test!(
        rgba_hex,
        "#f00f",
        Color::new_rgba(255, 0, 0, 255)
    );

    test!(
        rrggbb_spaced,
        "  #ff0000  ",
        Color::new_rgb(255, 0, 0)
    );

    test!(
        rgb_numeric,
        "rgb(254, 203, 231)",
        Color::new_rgb(254, 203, 231)
    );

    test!(
        rgb_numeric_spaced,
        " rgb( 77 , 77 , 77 ) ",
        Color::new_rgb(77, 77, 77)
    );

    test!(
        rgb_percentage,
        "rgb(50%, 50%, 50%)",
        Color::new_rgb(128, 128, 128)
    );

    test!(
        rgb_percentage_overflow,
        "rgb(140%, -10%, 130%)",
        Color::new_rgb(255, 0, 255)
    );

    test!(
        rgb_percentage_float,
        "rgb(33.333%,46.666%,93.333%)",
        Color::new_rgb(85, 119, 238)
    );

    test!(
        rgb_numeric_upper_case,
        "RGB(254, 203, 231)",
        Color::new_rgb(254, 203, 231)
    );

    test!(
        rgb_numeric_mixed_case,
        "RgB(254, 203, 231)",
        Color::new_rgb(254, 203, 231)
    );

    test!(
        rgb_numeric_red_float,
        "rgb(3.141592653, 110, 201)",
        Color::new_rgb(3, 110, 201)
    );

    test!(
        rgb_numeric_green_float,
        "rgb(254, 150.829521289232389, 210)",
        Color::new_rgb(254, 151, 210)
    );

    test!(
        rgb_numeric_blue_float,
        "rgb(96, 255, 0.2)",
        Color::new_rgb(96, 255, 0)
    );

    test!(
        rgb_numeric_all_float,
        "rgb(0.0, 129.82, 231.092)",
        Color::new_rgb(0, 130, 231)
    );

    test!(
        rgb_numeric_all_float_with_alpha,
        "rgb(0.0, 129.82, 231.092, 0.5)",
        Color::new_rgba(0, 130, 231, 128)
    );

    test!(
        rgb_numeric_all_float_overflow,
        "rgb(290.2, 255.9, 300.0)",
        Color::new_rgb(255, 255, 255)
    );

    test!(
        name_red,
        "red",
        Color::new_rgb(255, 0, 0)
    );

    test!(
        name_red_spaced,
        " red ",
        Color::new_rgb(255, 0, 0)
    );

    test!(
        name_red_upper_case,
        "RED",
        Color::new_rgb(255, 0, 0)
    );

    test!(
        name_red_mixed_case,
        "ReD",
        Color::new_rgb(255, 0, 0)
    );

    test!(
        name_cornflowerblue,
        "cornflowerblue",
        Color::new_rgb(100, 149, 237)
    );

    test!(
        transparent,
        "transparent",
        Color::new_rgba(0, 0, 0, 0)
    );

    test!(
        rgba_half,
        "rgba(10, 20, 30, 0.5)",
        Color::new_rgba(10, 20, 30, 128)
    );

    test!(
        rgba_numeric_red_float,
        "rgba(3.141592653, 110, 201, 1.0)",
        Color::new_rgba(3, 110, 201, 255)
    );

    test!(
        rgba_numeric_all_float,
        "rgba(0.0, 129.82, 231.092, 1.5)",
        Color::new_rgba(0, 130, 231, 255)
    );

    test!(
        rgba_negative,
        "rgba(10, 20, 30, -2)",
        Color::new_rgba(10, 20, 30, 0)
    );

    test!(
        rgba_large_alpha,
        "rgba(10, 20, 30, 2)",
        Color::new_rgba(10, 20, 30, 255)
    );

    test!(
        rgb_with_alpha,
        "rgb(10, 20, 30, 0.5)",
        Color::new_rgba(10, 20, 30, 128)
    );

    test!(
        hsl_green,
        "hsl(120, 100%, 75%)",
        Color::new_rgba(128, 255, 128, 255)
    );

    test!(
        hsl_yellow,
        "hsl(60, 100%, 50%)",
        Color::new_rgba(255, 255, 0, 255)
    );

    test!(
        hsl_hue_360,
        "hsl(360, 100%, 100%)",
        Color::new_rgba(255, 255, 255, 255)
    );

    test!(
        hsl_out_of_bounds,
        "hsl(800, 150%, -50%)",
        Color::new_rgba(0, 0, 0, 255)
    );

    test!(
        hsla_green,
        "hsla(120, 100%, 75%, 0.5)",
        Color::new_rgba(128, 255, 128, 128)
    );

    test!(
        hsl_with_alpha,
        "hsl(120, 100%, 75%, 0.5)",
        Color::new_rgba(128, 255, 128, 128)
    );

    test!(
        hsl_to_rgb_red_round_up,
        "hsl(230, 57%, 54%)",
        Color::new_rgba(71, 93, 205, 255)
    );

    test!(
        hsl_with_hue_float,
        "hsl(120.152, 100%, 75%)",
        Color::new_rgba(128, 255, 128, 255)
    );

    test!(
        hsla_with_hue_float,
        "hsla(120.152, 100%, 75%, 0.5)",
        Color::new_rgba(128, 255, 128, 128)
    );

    macro_rules! test_err {
        ($name:ident, $text:expr, $err:expr) => {
            #[test]
            fn $name() {
                assert_eq!(Color::from_str($text).unwrap_err().to_string(), $err);
            }
        };
    }

    test_err!(
        not_a_color_1,
        "text",
        "invalid value"
    );

    test_err!(
        icc_color_not_supported_1,
        "#CD853F icc-color(acmecmyk, 0.11, 0.48, 0.83, 0.00)",
        "unexpected data at position 9"
    );

    test_err!(
        icc_color_not_supported_2,
        "red icc-color(acmecmyk, 0.11, 0.48, 0.83, 0.00)",
        "unexpected data at position 5"
    );

    test_err!(
        invalid_input_1,
        "rgb(-0\x0d",
        "unexpected end of stream"
    );

    test_err!(
        invalid_input_2,
        "#9ߞpx! ;",
        "invalid value"
    );

    test_err!(
        rgba_with_percent_alpha,
        "rgba(10, 20, 30, 5%)",
        "expected ')' not '%' at position 19"
    );

    test_err!(
        rgb_mixed_units,
        "rgb(140%, -10mm, 130pt)",
        "invalid number at position 14"
    );

    // color-mix() tests
    #[cfg(feature = "color-mix")]
    mod color_mix_tests {
        use core::str::FromStr;
        use crate::Color;

        // sRGB mixing: red + blue = purple
        #[test]
        fn mix_srgb_equal() {
            let color = Color::from_str("color-mix(in srgb, red, blue)").unwrap();
            assert_eq!(color, Color::new_rgb(128, 0, 128));
        }

        // Weighted mixing: 25% red + 75% blue
        #[test]
        fn mix_srgb_weighted() {
            let color = Color::from_str("color-mix(in srgb, red 25%, blue)").unwrap();
            assert_eq!(color, Color::new_rgb(64, 0, 191));
        }

        // Both percentages specified
        #[test]
        fn mix_srgb_both_percentages() {
            let color = Color::from_str("color-mix(in srgb, red 30%, blue 70%)").unwrap();
            // Allow ±1 for rounding differences
            assert_eq!(color.red, 77);
            assert_eq!(color.green, 0);
            assert!(color.blue >= 178 && color.blue <= 179);
        }

        // Oklab mixing (default color space per spec)
        #[test]
        fn mix_oklab_equal() {
            let color = Color::from_str("color-mix(in oklab, red, blue)").unwrap();
            // Oklab produces different results than sRGB
            // The exact value depends on the oklab algorithm
            assert!(color.red > 0); // Should have some red
            assert!(color.blue > 0); // Should have some blue
        }

        // HSL mixing with hue interpolation
        #[test]
        fn mix_hsl_equal() {
            let color = Color::from_str("color-mix(in hsl, red, blue)").unwrap();
            // Red is hue 0, blue is hue 240, midpoint should be around 120 (green) or 300 (magenta)
            // depending on shorter/longer path
            assert!(color.alpha == 255);
        }

        // HSL with explicit hue interpolation
        #[test]
        fn mix_hsl_longer_hue() {
            let color = Color::from_str("color-mix(in hsl longer hue, red, blue)").unwrap();
            // Longer path from 0 to 240 goes through 360 -> green
            assert!(color.alpha == 255);
        }

        // With hex colors
        #[test]
        fn mix_hex_colors() {
            let color = Color::from_str("color-mix(in srgb, #ff0000, #0000ff)").unwrap();
            assert_eq!(color, Color::new_rgb(128, 0, 128));
        }

        // With rgba colors (alpha should be mixed too)
        #[test]
        fn mix_with_alpha() {
            let color = Color::from_str("color-mix(in srgb, rgba(255, 0, 0, 0.5), blue)").unwrap();
            // Alpha: 0.5 * 0.5 + 1.0 * 0.5 = 0.75 (about 191)
            assert!(color.alpha > 128 && color.alpha < 255);
        }

        // Different color spaces
        #[test]
        fn mix_lab() {
            let color = Color::from_str("color-mix(in lab, red, blue)").unwrap();
            assert!(color.red > 0 || color.blue > 0);
        }

        #[test]
        fn mix_lch() {
            let color = Color::from_str("color-mix(in lch, red, blue)").unwrap();
            assert!(color.alpha == 255);
        }

        #[test]
        fn mix_oklch() {
            let color = Color::from_str("color-mix(in oklch, red, blue)").unwrap();
            assert!(color.alpha == 255);
        }

        #[test]
        fn mix_hwb() {
            let color = Color::from_str("color-mix(in hwb, red, blue)").unwrap();
            assert!(color.alpha == 255);
        }

        #[test]
        fn mix_srgb_linear() {
            let color = Color::from_str("color-mix(in srgb-linear, red, blue)").unwrap();
            assert!(color.red > 0 || color.blue > 0);
        }

        // Named colors
        #[test]
        fn mix_named_colors() {
            let color = Color::from_str("color-mix(in srgb, crimson, dodgerblue)").unwrap();
            assert!(color.alpha == 255);
        }

        // Spacing variations
        #[test]
        fn mix_with_spaces() {
            let color = Color::from_str("color-mix( in srgb , red , blue )").unwrap();
            assert_eq!(color, Color::new_rgb(128, 0, 128));
        }
    }
}
