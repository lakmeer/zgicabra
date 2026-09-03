use rgb::RGB8;
use drawille::PixelColor::TrueColor;

pub const BLACK: RGB8 = RGB8 { r: 0, g: 0, b: 0 };
pub const WHITE: RGB8 = RGB8 { r: 255, g: 255, b: 255 };

pub const RED_50: RGB8 = RGB8 { r: 254, g: 242, b: 242 };
pub const RED_100: RGB8 = RGB8 { r: 255, g: 226, b: 226 };
pub const RED_200: RGB8 = RGB8 { r: 255, g: 201, b: 201 };
pub const RED_300: RGB8 = RGB8 { r: 255, g: 162, b: 162 };
pub const RED_400: RGB8 = RGB8 { r: 255, g: 100, b: 103 };
pub const RED_500: RGB8 = RGB8 { r: 251, g: 44, b: 54 };
pub const RED_600: RGB8 = RGB8 { r: 231, g: 0, b: 11 };
pub const RED_700: RGB8 = RGB8 { r: 193, g: 0, b: 7 };
pub const RED_800: RGB8 = RGB8 { r: 159, g: 7, b: 18 };
pub const RED_900: RGB8 = RGB8 { r: 130, g: 24, b: 26 };
pub const RED_950: RGB8 = RGB8 { r: 70, g: 8, b: 9 };

pub const ORANGE_50: RGB8 = RGB8 { r: 255, g: 247, b: 237 };
pub const ORANGE_100: RGB8 = RGB8 { r: 255, g: 237, b: 212 };
pub const ORANGE_200: RGB8 = RGB8 { r: 255, g: 214, b: 167 };
pub const ORANGE_300: RGB8 = RGB8 { r: 255, g: 184, b: 106 };
pub const ORANGE_400: RGB8 = RGB8 { r: 255, g: 137, b: 4 };
pub const ORANGE_500: RGB8 = RGB8 { r: 255, g: 105, b: 0 };
pub const ORANGE_600: RGB8 = RGB8 { r: 245, g: 73, b: 0 };
pub const ORANGE_700: RGB8 = RGB8 { r: 202, g: 53, b: 0 };
pub const ORANGE_800: RGB8 = RGB8 { r: 159, g: 45, b: 0 };
pub const ORANGE_900: RGB8 = RGB8 { r: 126, g: 42, b: 12 };
pub const ORANGE_950: RGB8 = RGB8 { r: 68, g: 19, b: 6 };

pub const AMBER_50: RGB8 = RGB8 { r: 255, g: 251, b: 235 };
pub const AMBER_100: RGB8 = RGB8 { r: 254, g: 243, b: 198 };
pub const AMBER_200: RGB8 = RGB8 { r: 254, g: 230, b: 133 };
pub const AMBER_300: RGB8 = RGB8 { r: 255, g: 210, b: 48 };
pub const AMBER_400: RGB8 = RGB8 { r: 255, g: 185, b: 0 };
pub const AMBER_500: RGB8 = RGB8 { r: 254, g: 154, b: 0 };
pub const AMBER_600: RGB8 = RGB8 { r: 225, g: 113, b: 0 };
pub const AMBER_700: RGB8 = RGB8 { r: 187, g: 77, b: 0 };
pub const AMBER_800: RGB8 = RGB8 { r: 151, g: 60, b: 0 };
pub const AMBER_900: RGB8 = RGB8 { r: 123, g: 51, b: 6 };
pub const AMBER_950: RGB8 = RGB8 { r: 70, g: 25, b: 1 };

pub const YELLOW_50: RGB8 = RGB8 { r: 254, g: 252, b: 232 };
pub const YELLOW_100: RGB8 = RGB8 { r: 254, g: 249, b: 194 };
pub const YELLOW_200: RGB8 = RGB8 { r: 255, g: 240, b: 133 };
pub const YELLOW_300: RGB8 = RGB8 { r: 255, g: 223, b: 32 };
pub const YELLOW_400: RGB8 = RGB8 { r: 253, g: 199, b: 0 };
pub const YELLOW_500: RGB8 = RGB8 { r: 240, g: 177, b: 0 };
pub const YELLOW_600: RGB8 = RGB8 { r: 208, g: 135, b: 0 };
pub const YELLOW_700: RGB8 = RGB8 { r: 166, g: 95, b: 0 };
pub const YELLOW_800: RGB8 = RGB8 { r: 137, g: 75, b: 0 };
pub const YELLOW_900: RGB8 = RGB8 { r: 115, g: 62, b: 10 };
pub const YELLOW_950: RGB8 = RGB8 { r: 67, g: 32, b: 4 };

pub const LIME_50: RGB8 = RGB8 { r: 247, g: 254, b: 231 };
pub const LIME_100: RGB8 = RGB8 { r: 236, g: 252, b: 202 };
pub const LIME_200: RGB8 = RGB8 { r: 216, g: 249, b: 153 };
pub const LIME_300: RGB8 = RGB8 { r: 187, g: 244, b: 81 };
pub const LIME_400: RGB8 = RGB8 { r: 154, g: 230, b: 0 };
pub const LIME_500: RGB8 = RGB8 { r: 124, g: 207, b: 0 };
pub const LIME_600: RGB8 = RGB8 { r: 94, g: 165, b: 0 };
pub const LIME_700: RGB8 = RGB8 { r: 73, g: 125, b: 0 };
pub const LIME_800: RGB8 = RGB8 { r: 60, g: 99, b: 0 };
pub const LIME_900: RGB8 = RGB8 { r: 53, g: 83, b: 14 };
pub const LIME_950: RGB8 = RGB8 { r: 25, g: 46, b: 3 };

pub const GREEN_50: RGB8 = RGB8 { r: 240, g: 253, b: 244 };
pub const GREEN_100: RGB8 = RGB8 { r: 220, g: 252, b: 231 };
pub const GREEN_200: RGB8 = RGB8 { r: 185, g: 248, b: 207 };
pub const GREEN_300: RGB8 = RGB8 { r: 123, g: 241, b: 168 };
pub const GREEN_400: RGB8 = RGB8 { r: 5, g: 223, b: 114 };
pub const GREEN_500: RGB8 = RGB8 { r: 0, g: 201, b: 80 };
pub const GREEN_600: RGB8 = RGB8 { r: 0, g: 166, b: 62 };
pub const GREEN_700: RGB8 = RGB8 { r: 0, g: 130, b: 54 };
pub const GREEN_800: RGB8 = RGB8 { r: 1, g: 102, b: 48 };
pub const GREEN_900: RGB8 = RGB8 { r: 13, g: 84, b: 43 };
pub const GREEN_950: RGB8 = RGB8 { r: 3, g: 46, b: 21 };

pub const EMERALD_50: RGB8 = RGB8 { r: 236, g: 253, b: 245 };
pub const EMERALD_100: RGB8 = RGB8 { r: 208, g: 250, b: 229 };
pub const EMERALD_200: RGB8 = RGB8 { r: 164, g: 244, b: 207 };
pub const EMERALD_300: RGB8 = RGB8 { r: 94, g: 233, b: 181 };
pub const EMERALD_400: RGB8 = RGB8 { r: 0, g: 212, b: 146 };
pub const EMERALD_500: RGB8 = RGB8 { r: 0, g: 188, b: 125 };
pub const EMERALD_600: RGB8 = RGB8 { r: 0, g: 153, b: 102 };
pub const EMERALD_700: RGB8 = RGB8 { r: 0, g: 122, b: 85 };
pub const EMERALD_800: RGB8 = RGB8 { r: 0, g: 96, b: 69 };
pub const EMERALD_900: RGB8 = RGB8 { r: 0, g: 79, b: 59 };
pub const EMERALD_950: RGB8 = RGB8 { r: 0, g: 44, b: 34 };

pub const TEAL_50: RGB8 = RGB8 { r: 240, g: 253, b: 250 };
pub const TEAL_100: RGB8 = RGB8 { r: 203, g: 251, b: 241 };
pub const TEAL_200: RGB8 = RGB8 { r: 150, g: 247, b: 228 };
pub const TEAL_300: RGB8 = RGB8 { r: 70, g: 236, b: 213 };
pub const TEAL_400: RGB8 = RGB8 { r: 0, g: 213, b: 190 };
pub const TEAL_500: RGB8 = RGB8 { r: 0, g: 187, b: 167 };
pub const TEAL_600: RGB8 = RGB8 { r: 0, g: 150, b: 137 };
pub const TEAL_700: RGB8 = RGB8 { r: 0, g: 120, b: 111 };
pub const TEAL_800: RGB8 = RGB8 { r: 0, g: 95, b: 90 };
pub const TEAL_900: RGB8 = RGB8 { r: 11, g: 79, b: 74 };
pub const TEAL_950: RGB8 = RGB8 { r: 2, g: 47, b: 46 };

pub const CYAN_50: RGB8 = RGB8 { r: 236, g: 254, b: 255 };
pub const CYAN_100: RGB8 = RGB8 { r: 206, g: 250, b: 254 };
pub const CYAN_200: RGB8 = RGB8 { r: 162, g: 244, b: 253 };
pub const CYAN_300: RGB8 = RGB8 { r: 83, g: 234, b: 253 };
pub const CYAN_400: RGB8 = RGB8 { r: 0, g: 211, b: 242 };
pub const CYAN_500: RGB8 = RGB8 { r: 0, g: 184, b: 219 };
pub const CYAN_600: RGB8 = RGB8 { r: 0, g: 146, b: 184 };
pub const CYAN_700: RGB8 = RGB8 { r: 0, g: 117, b: 149 };
pub const CYAN_800: RGB8 = RGB8 { r: 0, g: 95, b: 120 };
pub const CYAN_900: RGB8 = RGB8 { r: 16, g: 78, b: 100 };
pub const CYAN_950: RGB8 = RGB8 { r: 5, g: 51, b: 69 };

pub const SKY_50: RGB8 = RGB8 { r: 240, g: 249, b: 255 };
pub const SKY_100: RGB8 = RGB8 { r: 223, g: 242, b: 254 };
pub const SKY_200: RGB8 = RGB8 { r: 184, g: 230, b: 254 };
pub const SKY_300: RGB8 = RGB8 { r: 116, g: 212, b: 255 };
pub const SKY_400: RGB8 = RGB8 { r: 0, g: 188, b: 255 };
pub const SKY_500: RGB8 = RGB8 { r: 0, g: 166, b: 244 };
pub const SKY_600: RGB8 = RGB8 { r: 0, g: 132, b: 209 };
pub const SKY_700: RGB8 = RGB8 { r: 0, g: 105, b: 168 };
pub const SKY_800: RGB8 = RGB8 { r: 0, g: 89, b: 138 };
pub const SKY_900: RGB8 = RGB8 { r: 2, g: 74, b: 112 };
pub const SKY_950: RGB8 = RGB8 { r: 5, g: 47, b: 74 };

pub const BLUE_50: RGB8 = RGB8 { r: 239, g: 246, b: 255 };
pub const BLUE_100: RGB8 = RGB8 { r: 219, g: 234, b: 254 };
pub const BLUE_200: RGB8 = RGB8 { r: 190, g: 219, b: 255 };
pub const BLUE_300: RGB8 = RGB8 { r: 142, g: 197, b: 255 };
pub const BLUE_400: RGB8 = RGB8 { r: 81, g: 162, b: 255 };
pub const BLUE_500: RGB8 = RGB8 { r: 43, g: 127, b: 255 };
pub const BLUE_600: RGB8 = RGB8 { r: 21, g: 93, b: 252 };
pub const BLUE_700: RGB8 = RGB8 { r: 20, g: 71, b: 230 };
pub const BLUE_800: RGB8 = RGB8 { r: 25, g: 60, b: 184 };
pub const BLUE_900: RGB8 = RGB8 { r: 28, g: 57, b: 142 };
pub const BLUE_950: RGB8 = RGB8 { r: 22, g: 36, b: 86 };

pub const INDIGO_50: RGB8 = RGB8 { r: 238, g: 242, b: 255 };
pub const INDIGO_100: RGB8 = RGB8 { r: 224, g: 231, b: 255 };
pub const INDIGO_200: RGB8 = RGB8 { r: 198, g: 210, b: 255 };
pub const INDIGO_300: RGB8 = RGB8 { r: 163, g: 179, b: 255 };
pub const INDIGO_400: RGB8 = RGB8 { r: 124, g: 134, b: 255 };
pub const INDIGO_500: RGB8 = RGB8 { r: 97, g: 95, b: 255 };
pub const INDIGO_600: RGB8 = RGB8 { r: 79, g: 57, b: 246 };
pub const INDIGO_700: RGB8 = RGB8 { r: 67, g: 45, b: 215 };
pub const INDIGO_800: RGB8 = RGB8 { r: 55, g: 42, b: 172 };
pub const INDIGO_900: RGB8 = RGB8 { r: 49, g: 44, b: 133 };
pub const INDIGO_950: RGB8 = RGB8 { r: 30, g: 26, b: 77 };

pub const VIOLET_50: RGB8 = RGB8 { r: 245, g: 243, b: 255 };
pub const VIOLET_100: RGB8 = RGB8 { r: 237, g: 233, b: 254 };
pub const VIOLET_200: RGB8 = RGB8 { r: 221, g: 214, b: 255 };
pub const VIOLET_300: RGB8 = RGB8 { r: 196, g: 180, b: 255 };
pub const VIOLET_400: RGB8 = RGB8 { r: 166, g: 132, b: 255 };
pub const VIOLET_500: RGB8 = RGB8 { r: 142, g: 81, b: 255 };
pub const VIOLET_600: RGB8 = RGB8 { r: 127, g: 34, b: 254 };
pub const VIOLET_700: RGB8 = RGB8 { r: 112, g: 8, b: 231 };
pub const VIOLET_800: RGB8 = RGB8 { r: 93, g: 14, b: 192 };
pub const VIOLET_900: RGB8 = RGB8 { r: 77, g: 23, b: 154 };
pub const VIOLET_950: RGB8 = RGB8 { r: 47, g: 13, b: 104 };

pub const PURPLE_50: RGB8 = RGB8 { r: 250, g: 245, b: 255 };
pub const PURPLE_100: RGB8 = RGB8 { r: 243, g: 232, b: 255 };
pub const PURPLE_200: RGB8 = RGB8 { r: 233, g: 212, b: 255 };
pub const PURPLE_300: RGB8 = RGB8 { r: 218, g: 178, b: 255 };
pub const PURPLE_400: RGB8 = RGB8 { r: 194, g: 122, b: 255 };
pub const PURPLE_500: RGB8 = RGB8 { r: 173, g: 70, b: 255 };
pub const PURPLE_600: RGB8 = RGB8 { r: 152, g: 16, b: 250 };
pub const PURPLE_700: RGB8 = RGB8 { r: 130, g: 0, b: 219 };
pub const PURPLE_800: RGB8 = RGB8 { r: 110, g: 17, b: 176 };
pub const PURPLE_900: RGB8 = RGB8 { r: 89, g: 22, b: 139 };
pub const PURPLE_950: RGB8 = RGB8 { r: 60, g: 3, b: 102 };

pub const FUCHSIA_50: RGB8 = RGB8 { r: 253, g: 244, b: 255 };
pub const FUCHSIA_100: RGB8 = RGB8 { r: 250, g: 232, b: 255 };
pub const FUCHSIA_200: RGB8 = RGB8 { r: 246, g: 207, b: 255 };
pub const FUCHSIA_300: RGB8 = RGB8 { r: 244, g: 168, b: 255 };
pub const FUCHSIA_400: RGB8 = RGB8 { r: 237, g: 106, b: 255 };
pub const FUCHSIA_500: RGB8 = RGB8 { r: 225, g: 42, b: 251 };
pub const FUCHSIA_600: RGB8 = RGB8 { r: 200, g: 0, b: 222 };
pub const FUCHSIA_700: RGB8 = RGB8 { r: 168, g: 0, b: 183 };
pub const FUCHSIA_800: RGB8 = RGB8 { r: 138, g: 1, b: 148 };
pub const FUCHSIA_900: RGB8 = RGB8 { r: 114, g: 19, b: 120 };
pub const FUCHSIA_950: RGB8 = RGB8 { r: 75, g: 0, b: 79 };

pub const PINK_50: RGB8 = RGB8 { r: 253, g: 242, b: 248 };
pub const PINK_100: RGB8 = RGB8 { r: 252, g: 231, b: 243 };
pub const PINK_200: RGB8 = RGB8 { r: 252, g: 206, b: 232 };
pub const PINK_300: RGB8 = RGB8 { r: 253, g: 165, b: 213 };
pub const PINK_400: RGB8 = RGB8 { r: 251, g: 100, b: 182 };
pub const PINK_500: RGB8 = RGB8 { r: 246, g: 51, b: 154 };
pub const PINK_600: RGB8 = RGB8 { r: 230, g: 0, b: 118 };
pub const PINK_700: RGB8 = RGB8 { r: 198, g: 0, b: 92 };
pub const PINK_800: RGB8 = RGB8 { r: 163, g: 0, b: 76 };
pub const PINK_900: RGB8 = RGB8 { r: 134, g: 16, b: 67 };
pub const PINK_950: RGB8 = RGB8 { r: 81, g: 4, b: 36 };

pub const ROSE_50: RGB8 = RGB8 { r: 255, g: 241, b: 242 };
pub const ROSE_100: RGB8 = RGB8 { r: 255, g: 228, b: 230 };
pub const ROSE_200: RGB8 = RGB8 { r: 255, g: 204, b: 211 };
pub const ROSE_300: RGB8 = RGB8 { r: 255, g: 161, b: 173 };
pub const ROSE_400: RGB8 = RGB8 { r: 255, g: 99, b: 126 };
pub const ROSE_500: RGB8 = RGB8 { r: 255, g: 32, b: 86 };
pub const ROSE_600: RGB8 = RGB8 { r: 236, g: 0, b: 63 };
pub const ROSE_700: RGB8 = RGB8 { r: 199, g: 0, b: 54 };
pub const ROSE_800: RGB8 = RGB8 { r: 165, g: 0, b: 54 };
pub const ROSE_900: RGB8 = RGB8 { r: 139, g: 8, b: 54 };
pub const ROSE_950: RGB8 = RGB8 { r: 77, g: 2, b: 24 };

pub const SLATE_50: RGB8 = RGB8 { r: 248, g: 250, b: 252 };
pub const SLATE_100: RGB8 = RGB8 { r: 241, g: 245, b: 249 };
pub const SLATE_200: RGB8 = RGB8 { r: 226, g: 232, b: 240 };
pub const SLATE_300: RGB8 = RGB8 { r: 202, g: 213, b: 226 };
pub const SLATE_400: RGB8 = RGB8 { r: 144, g: 161, b: 185 };
pub const SLATE_500: RGB8 = RGB8 { r: 98, g: 116, b: 142 };
pub const SLATE_600: RGB8 = RGB8 { r: 69, g: 85, b: 108 };
pub const SLATE_700: RGB8 = RGB8 { r: 49, g: 65, b: 88 };
pub const SLATE_800: RGB8 = RGB8 { r: 29, g: 41, b: 61 };
pub const SLATE_900: RGB8 = RGB8 { r: 15, g: 23, b: 43 };
pub const SLATE_950: RGB8 = RGB8 { r: 2, g: 6, b: 24 };

pub const GRAY_50: RGB8 = RGB8 { r: 249, g: 250, b: 251 };
pub const GRAY_100: RGB8 = RGB8 { r: 243, g: 244, b: 246 };
pub const GRAY_200: RGB8 = RGB8 { r: 229, g: 231, b: 235 };
pub const GRAY_300: RGB8 = RGB8 { r: 209, g: 213, b: 220 };
pub const GRAY_400: RGB8 = RGB8 { r: 153, g: 161, b: 175 };
pub const GRAY_500: RGB8 = RGB8 { r: 106, g: 114, b: 130 };
pub const GRAY_600: RGB8 = RGB8 { r: 74, g: 85, b: 101 };
pub const GRAY_700: RGB8 = RGB8 { r: 54, g: 65, b: 83 };
pub const GRAY_800: RGB8 = RGB8 { r: 30, g: 41, b: 57 };
pub const GRAY_900: RGB8 = RGB8 { r: 16, g: 24, b: 40 };
pub const GRAY_950: RGB8 = RGB8 { r: 3, g: 7, b: 18 };

pub const ZINC_50: RGB8 = RGB8 { r: 250, g: 250, b: 250 };
pub const ZINC_100: RGB8 = RGB8 { r: 244, g: 244, b: 245 };
pub const ZINC_200: RGB8 = RGB8 { r: 228, g: 228, b: 231 };
pub const ZINC_300: RGB8 = RGB8 { r: 212, g: 212, b: 216 };
pub const ZINC_400: RGB8 = RGB8 { r: 159, g: 159, b: 169 };
pub const ZINC_500: RGB8 = RGB8 { r: 113, g: 113, b: 123 };
pub const ZINC_600: RGB8 = RGB8 { r: 82, g: 82, b: 92 };
pub const ZINC_700: RGB8 = RGB8 { r: 63, g: 63, b: 70 };
pub const ZINC_800: RGB8 = RGB8 { r: 39, g: 39, b: 42 };
pub const ZINC_900: RGB8 = RGB8 { r: 24, g: 24, b: 27 };
pub const ZINC_950: RGB8 = RGB8 { r: 9, g: 9, b: 11 };

pub const NEUTRAL_50: RGB8 = RGB8 { r: 250, g: 250, b: 250 };
pub const NEUTRAL_100: RGB8 = RGB8 { r: 245, g: 245, b: 245 };
pub const NEUTRAL_200: RGB8 = RGB8 { r: 229, g: 229, b: 229 };
pub const NEUTRAL_300: RGB8 = RGB8 { r: 212, g: 212, b: 212 };
pub const NEUTRAL_400: RGB8 = RGB8 { r: 161, g: 161, b: 161 };
pub const NEUTRAL_500: RGB8 = RGB8 { r: 115, g: 115, b: 115 };
pub const NEUTRAL_600: RGB8 = RGB8 { r: 82, g: 82, b: 82 };
pub const NEUTRAL_700: RGB8 = RGB8 { r: 64, g: 64, b: 64 };
pub const NEUTRAL_800: RGB8 = RGB8 { r: 38, g: 38, b: 38 };
pub const NEUTRAL_900: RGB8 = RGB8 { r: 23, g: 23, b: 23 };
pub const NEUTRAL_950: RGB8 = RGB8 { r: 10, g: 10, b: 10 };

pub const STONE_50: RGB8 = RGB8 { r: 250, g: 250, b: 249 };
pub const STONE_100: RGB8 = RGB8 { r: 245, g: 245, b: 244 };
pub const STONE_200: RGB8 = RGB8 { r: 231, g: 229, b: 228 };
pub const STONE_300: RGB8 = RGB8 { r: 214, g: 211, b: 209 };
pub const STONE_400: RGB8 = RGB8 { r: 166, g: 160, b: 155 };
pub const STONE_500: RGB8 = RGB8 { r: 121, g: 113, b: 107 };
pub const STONE_600: RGB8 = RGB8 { r: 87, g: 83, b: 77 };
pub const STONE_700: RGB8 = RGB8 { r: 68, g: 64, b: 59 };
pub const STONE_800: RGB8 = RGB8 { r: 41, g: 37, b: 36 };
pub const STONE_900: RGB8 = RGB8 { r: 28, g: 25, b: 23 };
pub const STONE_950: RGB8 = RGB8 { r: 12, g: 10, b: 9 };

pub const fn tw_rgb (rgb: RGB8) -> drawille::PixelColor {
    TrueColor {
        r: rgb.r as u8,
        g: rgb.g as u8,
        b: rgb.b as u8,
    }
}

