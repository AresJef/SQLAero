//! Factual catalogs queried from MySQL 8.4.13 and 9.7.4; both were identical.
//! No MySQL source tables were copied. Provenance hashes are recorded in
//! docs/charset-implementation-routes.md and the startup catalog evidence.

use super::{Charset, CodecRoute, Collation, Multibyte};

pub(super) static CHARSETS: &[Charset] = &[
    Charset {
        name: "armscii8",
        default_collation: "armscii8_general_ci",
        max_width: 1,
        route: CodecRoute::SingleByte(0),
    },
    Charset {
        name: "ascii",
        default_collation: "ascii_general_ci",
        max_width: 1,
        route: CodecRoute::Ascii,
    },
    Charset {
        name: "big5",
        default_collation: "big5_chinese_ci",
        max_width: 2,
        route: CodecRoute::Multibyte(Multibyte::Big5),
    },
    Charset {
        name: "binary",
        default_collation: "binary",
        max_width: 1,
        route: CodecRoute::Binary,
    },
    Charset {
        name: "cp1250",
        default_collation: "cp1250_general_ci",
        max_width: 1,
        route: CodecRoute::SingleByte(1),
    },
    Charset {
        name: "cp1251",
        default_collation: "cp1251_general_ci",
        max_width: 1,
        route: CodecRoute::SingleByte(2),
    },
    Charset {
        name: "cp1256",
        default_collation: "cp1256_general_ci",
        max_width: 1,
        route: CodecRoute::SingleByte(3),
    },
    Charset {
        name: "cp1257",
        default_collation: "cp1257_general_ci",
        max_width: 1,
        route: CodecRoute::SingleByte(4),
    },
    Charset {
        name: "cp850",
        default_collation: "cp850_general_ci",
        max_width: 1,
        route: CodecRoute::SingleByte(5),
    },
    Charset {
        name: "cp852",
        default_collation: "cp852_general_ci",
        max_width: 1,
        route: CodecRoute::SingleByte(6),
    },
    Charset {
        name: "cp866",
        default_collation: "cp866_general_ci",
        max_width: 1,
        route: CodecRoute::SingleByte(7),
    },
    Charset {
        name: "cp932",
        default_collation: "cp932_japanese_ci",
        max_width: 2,
        route: CodecRoute::Multibyte(Multibyte::Cp932),
    },
    Charset {
        name: "dec8",
        default_collation: "dec8_swedish_ci",
        max_width: 1,
        route: CodecRoute::SingleByte(8),
    },
    Charset {
        name: "eucjpms",
        default_collation: "eucjpms_japanese_ci",
        max_width: 3,
        route: CodecRoute::Multibyte(Multibyte::Eucjpms),
    },
    Charset {
        name: "euckr",
        default_collation: "euckr_korean_ci",
        max_width: 2,
        route: CodecRoute::Multibyte(Multibyte::Euckr),
    },
    Charset {
        name: "gb18030",
        default_collation: "gb18030_chinese_ci",
        max_width: 4,
        route: CodecRoute::Multibyte(Multibyte::Gb18030),
    },
    Charset {
        name: "gb2312",
        default_collation: "gb2312_chinese_ci",
        max_width: 2,
        route: CodecRoute::Multibyte(Multibyte::Gb2312),
    },
    Charset {
        name: "gbk",
        default_collation: "gbk_chinese_ci",
        max_width: 2,
        route: CodecRoute::Multibyte(Multibyte::Gbk),
    },
    Charset {
        name: "geostd8",
        default_collation: "geostd8_general_ci",
        max_width: 1,
        route: CodecRoute::SingleByte(9),
    },
    Charset {
        name: "greek",
        default_collation: "greek_general_ci",
        max_width: 1,
        route: CodecRoute::SingleByte(10),
    },
    Charset {
        name: "hebrew",
        default_collation: "hebrew_general_ci",
        max_width: 1,
        route: CodecRoute::SingleByte(11),
    },
    Charset {
        name: "hp8",
        default_collation: "hp8_english_ci",
        max_width: 1,
        route: CodecRoute::SingleByte(12),
    },
    Charset {
        name: "keybcs2",
        default_collation: "keybcs2_general_ci",
        max_width: 1,
        route: CodecRoute::SingleByte(13),
    },
    Charset {
        name: "koi8r",
        default_collation: "koi8r_general_ci",
        max_width: 1,
        route: CodecRoute::SingleByte(14),
    },
    Charset {
        name: "koi8u",
        default_collation: "koi8u_general_ci",
        max_width: 1,
        route: CodecRoute::SingleByte(15),
    },
    Charset {
        name: "latin1",
        default_collation: "latin1_swedish_ci",
        max_width: 1,
        route: CodecRoute::SingleByte(16),
    },
    Charset {
        name: "latin2",
        default_collation: "latin2_general_ci",
        max_width: 1,
        route: CodecRoute::SingleByte(17),
    },
    Charset {
        name: "latin5",
        default_collation: "latin5_turkish_ci",
        max_width: 1,
        route: CodecRoute::SingleByte(18),
    },
    Charset {
        name: "latin7",
        default_collation: "latin7_general_ci",
        max_width: 1,
        route: CodecRoute::SingleByte(19),
    },
    Charset {
        name: "macce",
        default_collation: "macce_general_ci",
        max_width: 1,
        route: CodecRoute::SingleByte(20),
    },
    Charset {
        name: "macroman",
        default_collation: "macroman_general_ci",
        max_width: 1,
        route: CodecRoute::SingleByte(21),
    },
    Charset {
        name: "sjis",
        default_collation: "sjis_japanese_ci",
        max_width: 2,
        route: CodecRoute::Multibyte(Multibyte::Sjis),
    },
    Charset {
        name: "swe7",
        default_collation: "swe7_swedish_ci",
        max_width: 1,
        route: CodecRoute::SingleByte(22),
    },
    Charset {
        name: "tis620",
        default_collation: "tis620_thai_ci",
        max_width: 1,
        route: CodecRoute::SingleByte(23),
    },
    Charset {
        name: "ucs2",
        default_collation: "ucs2_general_ci",
        max_width: 2,
        route: CodecRoute::Unavailable,
    },
    Charset {
        name: "ujis",
        default_collation: "ujis_japanese_ci",
        max_width: 3,
        route: CodecRoute::Multibyte(Multibyte::Ujis),
    },
    Charset {
        name: "utf16",
        default_collation: "utf16_general_ci",
        max_width: 4,
        route: CodecRoute::Unavailable,
    },
    Charset {
        name: "utf16le",
        default_collation: "utf16le_general_ci",
        max_width: 4,
        route: CodecRoute::Unavailable,
    },
    Charset {
        name: "utf32",
        default_collation: "utf32_general_ci",
        max_width: 4,
        route: CodecRoute::Unavailable,
    },
    Charset {
        name: "utf8mb3",
        default_collation: "utf8mb3_general_ci",
        max_width: 3,
        route: CodecRoute::Unavailable,
    },
    Charset {
        name: "utf8mb4",
        default_collation: "utf8mb4_0900_ai_ci",
        max_width: 4,
        route: CodecRoute::Utf8,
    },
];

pub(super) static COLLATIONS: &[Collation] = &[
    Collation {
        name: "armscii8_bin",
        charset: "armscii8",
        id: 64,
    },
    Collation {
        name: "armscii8_general_ci",
        charset: "armscii8",
        id: 32,
    },
    Collation {
        name: "ascii_bin",
        charset: "ascii",
        id: 65,
    },
    Collation {
        name: "ascii_general_ci",
        charset: "ascii",
        id: 11,
    },
    Collation {
        name: "big5_bin",
        charset: "big5",
        id: 84,
    },
    Collation {
        name: "big5_chinese_ci",
        charset: "big5",
        id: 1,
    },
    Collation {
        name: "binary",
        charset: "binary",
        id: 63,
    },
    Collation {
        name: "cp1250_bin",
        charset: "cp1250",
        id: 66,
    },
    Collation {
        name: "cp1250_croatian_ci",
        charset: "cp1250",
        id: 44,
    },
    Collation {
        name: "cp1250_czech_cs",
        charset: "cp1250",
        id: 34,
    },
    Collation {
        name: "cp1250_general_ci",
        charset: "cp1250",
        id: 26,
    },
    Collation {
        name: "cp1250_polish_ci",
        charset: "cp1250",
        id: 99,
    },
    Collation {
        name: "cp1251_bin",
        charset: "cp1251",
        id: 50,
    },
    Collation {
        name: "cp1251_bulgarian_ci",
        charset: "cp1251",
        id: 14,
    },
    Collation {
        name: "cp1251_general_ci",
        charset: "cp1251",
        id: 51,
    },
    Collation {
        name: "cp1251_general_cs",
        charset: "cp1251",
        id: 52,
    },
    Collation {
        name: "cp1251_ukrainian_ci",
        charset: "cp1251",
        id: 23,
    },
    Collation {
        name: "cp1256_bin",
        charset: "cp1256",
        id: 67,
    },
    Collation {
        name: "cp1256_general_ci",
        charset: "cp1256",
        id: 57,
    },
    Collation {
        name: "cp1257_bin",
        charset: "cp1257",
        id: 58,
    },
    Collation {
        name: "cp1257_general_ci",
        charset: "cp1257",
        id: 59,
    },
    Collation {
        name: "cp1257_lithuanian_ci",
        charset: "cp1257",
        id: 29,
    },
    Collation {
        name: "cp850_bin",
        charset: "cp850",
        id: 80,
    },
    Collation {
        name: "cp850_general_ci",
        charset: "cp850",
        id: 4,
    },
    Collation {
        name: "cp852_bin",
        charset: "cp852",
        id: 81,
    },
    Collation {
        name: "cp852_general_ci",
        charset: "cp852",
        id: 40,
    },
    Collation {
        name: "cp866_bin",
        charset: "cp866",
        id: 68,
    },
    Collation {
        name: "cp866_general_ci",
        charset: "cp866",
        id: 36,
    },
    Collation {
        name: "cp932_bin",
        charset: "cp932",
        id: 96,
    },
    Collation {
        name: "cp932_japanese_ci",
        charset: "cp932",
        id: 95,
    },
    Collation {
        name: "dec8_bin",
        charset: "dec8",
        id: 69,
    },
    Collation {
        name: "dec8_swedish_ci",
        charset: "dec8",
        id: 3,
    },
    Collation {
        name: "eucjpms_bin",
        charset: "eucjpms",
        id: 98,
    },
    Collation {
        name: "eucjpms_japanese_ci",
        charset: "eucjpms",
        id: 97,
    },
    Collation {
        name: "euckr_bin",
        charset: "euckr",
        id: 85,
    },
    Collation {
        name: "euckr_korean_ci",
        charset: "euckr",
        id: 19,
    },
    Collation {
        name: "gb18030_bin",
        charset: "gb18030",
        id: 249,
    },
    Collation {
        name: "gb18030_chinese_ci",
        charset: "gb18030",
        id: 248,
    },
    Collation {
        name: "gb18030_unicode_520_ci",
        charset: "gb18030",
        id: 250,
    },
    Collation {
        name: "gb2312_bin",
        charset: "gb2312",
        id: 86,
    },
    Collation {
        name: "gb2312_chinese_ci",
        charset: "gb2312",
        id: 24,
    },
    Collation {
        name: "gbk_bin",
        charset: "gbk",
        id: 87,
    },
    Collation {
        name: "gbk_chinese_ci",
        charset: "gbk",
        id: 28,
    },
    Collation {
        name: "geostd8_bin",
        charset: "geostd8",
        id: 93,
    },
    Collation {
        name: "geostd8_general_ci",
        charset: "geostd8",
        id: 92,
    },
    Collation {
        name: "greek_bin",
        charset: "greek",
        id: 70,
    },
    Collation {
        name: "greek_general_ci",
        charset: "greek",
        id: 25,
    },
    Collation {
        name: "hebrew_bin",
        charset: "hebrew",
        id: 71,
    },
    Collation {
        name: "hebrew_general_ci",
        charset: "hebrew",
        id: 16,
    },
    Collation {
        name: "hp8_bin",
        charset: "hp8",
        id: 72,
    },
    Collation {
        name: "hp8_english_ci",
        charset: "hp8",
        id: 6,
    },
    Collation {
        name: "keybcs2_bin",
        charset: "keybcs2",
        id: 73,
    },
    Collation {
        name: "keybcs2_general_ci",
        charset: "keybcs2",
        id: 37,
    },
    Collation {
        name: "koi8r_bin",
        charset: "koi8r",
        id: 74,
    },
    Collation {
        name: "koi8r_general_ci",
        charset: "koi8r",
        id: 7,
    },
    Collation {
        name: "koi8u_bin",
        charset: "koi8u",
        id: 75,
    },
    Collation {
        name: "koi8u_general_ci",
        charset: "koi8u",
        id: 22,
    },
    Collation {
        name: "latin1_bin",
        charset: "latin1",
        id: 47,
    },
    Collation {
        name: "latin1_danish_ci",
        charset: "latin1",
        id: 15,
    },
    Collation {
        name: "latin1_general_ci",
        charset: "latin1",
        id: 48,
    },
    Collation {
        name: "latin1_general_cs",
        charset: "latin1",
        id: 49,
    },
    Collation {
        name: "latin1_german1_ci",
        charset: "latin1",
        id: 5,
    },
    Collation {
        name: "latin1_german2_ci",
        charset: "latin1",
        id: 31,
    },
    Collation {
        name: "latin1_spanish_ci",
        charset: "latin1",
        id: 94,
    },
    Collation {
        name: "latin1_swedish_ci",
        charset: "latin1",
        id: 8,
    },
    Collation {
        name: "latin2_bin",
        charset: "latin2",
        id: 77,
    },
    Collation {
        name: "latin2_croatian_ci",
        charset: "latin2",
        id: 27,
    },
    Collation {
        name: "latin2_czech_cs",
        charset: "latin2",
        id: 2,
    },
    Collation {
        name: "latin2_general_ci",
        charset: "latin2",
        id: 9,
    },
    Collation {
        name: "latin2_hungarian_ci",
        charset: "latin2",
        id: 21,
    },
    Collation {
        name: "latin5_bin",
        charset: "latin5",
        id: 78,
    },
    Collation {
        name: "latin5_turkish_ci",
        charset: "latin5",
        id: 30,
    },
    Collation {
        name: "latin7_bin",
        charset: "latin7",
        id: 79,
    },
    Collation {
        name: "latin7_estonian_cs",
        charset: "latin7",
        id: 20,
    },
    Collation {
        name: "latin7_general_ci",
        charset: "latin7",
        id: 41,
    },
    Collation {
        name: "latin7_general_cs",
        charset: "latin7",
        id: 42,
    },
    Collation {
        name: "macce_bin",
        charset: "macce",
        id: 43,
    },
    Collation {
        name: "macce_general_ci",
        charset: "macce",
        id: 38,
    },
    Collation {
        name: "macroman_bin",
        charset: "macroman",
        id: 53,
    },
    Collation {
        name: "macroman_general_ci",
        charset: "macroman",
        id: 39,
    },
    Collation {
        name: "sjis_bin",
        charset: "sjis",
        id: 88,
    },
    Collation {
        name: "sjis_japanese_ci",
        charset: "sjis",
        id: 13,
    },
    Collation {
        name: "swe7_bin",
        charset: "swe7",
        id: 82,
    },
    Collation {
        name: "swe7_swedish_ci",
        charset: "swe7",
        id: 10,
    },
    Collation {
        name: "tis620_bin",
        charset: "tis620",
        id: 89,
    },
    Collation {
        name: "tis620_thai_ci",
        charset: "tis620",
        id: 18,
    },
    Collation {
        name: "ucs2_bin",
        charset: "ucs2",
        id: 90,
    },
    Collation {
        name: "ucs2_croatian_ci",
        charset: "ucs2",
        id: 149,
    },
    Collation {
        name: "ucs2_czech_ci",
        charset: "ucs2",
        id: 138,
    },
    Collation {
        name: "ucs2_danish_ci",
        charset: "ucs2",
        id: 139,
    },
    Collation {
        name: "ucs2_esperanto_ci",
        charset: "ucs2",
        id: 145,
    },
    Collation {
        name: "ucs2_estonian_ci",
        charset: "ucs2",
        id: 134,
    },
    Collation {
        name: "ucs2_general_ci",
        charset: "ucs2",
        id: 35,
    },
    Collation {
        name: "ucs2_general_mysql500_ci",
        charset: "ucs2",
        id: 159,
    },
    Collation {
        name: "ucs2_german2_ci",
        charset: "ucs2",
        id: 148,
    },
    Collation {
        name: "ucs2_hungarian_ci",
        charset: "ucs2",
        id: 146,
    },
    Collation {
        name: "ucs2_icelandic_ci",
        charset: "ucs2",
        id: 129,
    },
    Collation {
        name: "ucs2_latvian_ci",
        charset: "ucs2",
        id: 130,
    },
    Collation {
        name: "ucs2_lithuanian_ci",
        charset: "ucs2",
        id: 140,
    },
    Collation {
        name: "ucs2_persian_ci",
        charset: "ucs2",
        id: 144,
    },
    Collation {
        name: "ucs2_polish_ci",
        charset: "ucs2",
        id: 133,
    },
    Collation {
        name: "ucs2_roman_ci",
        charset: "ucs2",
        id: 143,
    },
    Collation {
        name: "ucs2_romanian_ci",
        charset: "ucs2",
        id: 131,
    },
    Collation {
        name: "ucs2_sinhala_ci",
        charset: "ucs2",
        id: 147,
    },
    Collation {
        name: "ucs2_slovak_ci",
        charset: "ucs2",
        id: 141,
    },
    Collation {
        name: "ucs2_slovenian_ci",
        charset: "ucs2",
        id: 132,
    },
    Collation {
        name: "ucs2_spanish2_ci",
        charset: "ucs2",
        id: 142,
    },
    Collation {
        name: "ucs2_spanish_ci",
        charset: "ucs2",
        id: 135,
    },
    Collation {
        name: "ucs2_swedish_ci",
        charset: "ucs2",
        id: 136,
    },
    Collation {
        name: "ucs2_turkish_ci",
        charset: "ucs2",
        id: 137,
    },
    Collation {
        name: "ucs2_unicode_520_ci",
        charset: "ucs2",
        id: 150,
    },
    Collation {
        name: "ucs2_unicode_ci",
        charset: "ucs2",
        id: 128,
    },
    Collation {
        name: "ucs2_vietnamese_ci",
        charset: "ucs2",
        id: 151,
    },
    Collation {
        name: "ujis_bin",
        charset: "ujis",
        id: 91,
    },
    Collation {
        name: "ujis_japanese_ci",
        charset: "ujis",
        id: 12,
    },
    Collation {
        name: "utf16_bin",
        charset: "utf16",
        id: 55,
    },
    Collation {
        name: "utf16_croatian_ci",
        charset: "utf16",
        id: 122,
    },
    Collation {
        name: "utf16_czech_ci",
        charset: "utf16",
        id: 111,
    },
    Collation {
        name: "utf16_danish_ci",
        charset: "utf16",
        id: 112,
    },
    Collation {
        name: "utf16_esperanto_ci",
        charset: "utf16",
        id: 118,
    },
    Collation {
        name: "utf16_estonian_ci",
        charset: "utf16",
        id: 107,
    },
    Collation {
        name: "utf16_general_ci",
        charset: "utf16",
        id: 54,
    },
    Collation {
        name: "utf16_german2_ci",
        charset: "utf16",
        id: 121,
    },
    Collation {
        name: "utf16_hungarian_ci",
        charset: "utf16",
        id: 119,
    },
    Collation {
        name: "utf16_icelandic_ci",
        charset: "utf16",
        id: 102,
    },
    Collation {
        name: "utf16_latvian_ci",
        charset: "utf16",
        id: 103,
    },
    Collation {
        name: "utf16_lithuanian_ci",
        charset: "utf16",
        id: 113,
    },
    Collation {
        name: "utf16_persian_ci",
        charset: "utf16",
        id: 117,
    },
    Collation {
        name: "utf16_polish_ci",
        charset: "utf16",
        id: 106,
    },
    Collation {
        name: "utf16_roman_ci",
        charset: "utf16",
        id: 116,
    },
    Collation {
        name: "utf16_romanian_ci",
        charset: "utf16",
        id: 104,
    },
    Collation {
        name: "utf16_sinhala_ci",
        charset: "utf16",
        id: 120,
    },
    Collation {
        name: "utf16_slovak_ci",
        charset: "utf16",
        id: 114,
    },
    Collation {
        name: "utf16_slovenian_ci",
        charset: "utf16",
        id: 105,
    },
    Collation {
        name: "utf16_spanish2_ci",
        charset: "utf16",
        id: 115,
    },
    Collation {
        name: "utf16_spanish_ci",
        charset: "utf16",
        id: 108,
    },
    Collation {
        name: "utf16_swedish_ci",
        charset: "utf16",
        id: 109,
    },
    Collation {
        name: "utf16_turkish_ci",
        charset: "utf16",
        id: 110,
    },
    Collation {
        name: "utf16_unicode_520_ci",
        charset: "utf16",
        id: 123,
    },
    Collation {
        name: "utf16_unicode_ci",
        charset: "utf16",
        id: 101,
    },
    Collation {
        name: "utf16_vietnamese_ci",
        charset: "utf16",
        id: 124,
    },
    Collation {
        name: "utf16le_bin",
        charset: "utf16le",
        id: 62,
    },
    Collation {
        name: "utf16le_general_ci",
        charset: "utf16le",
        id: 56,
    },
    Collation {
        name: "utf32_bin",
        charset: "utf32",
        id: 61,
    },
    Collation {
        name: "utf32_croatian_ci",
        charset: "utf32",
        id: 181,
    },
    Collation {
        name: "utf32_czech_ci",
        charset: "utf32",
        id: 170,
    },
    Collation {
        name: "utf32_danish_ci",
        charset: "utf32",
        id: 171,
    },
    Collation {
        name: "utf32_esperanto_ci",
        charset: "utf32",
        id: 177,
    },
    Collation {
        name: "utf32_estonian_ci",
        charset: "utf32",
        id: 166,
    },
    Collation {
        name: "utf32_general_ci",
        charset: "utf32",
        id: 60,
    },
    Collation {
        name: "utf32_german2_ci",
        charset: "utf32",
        id: 180,
    },
    Collation {
        name: "utf32_hungarian_ci",
        charset: "utf32",
        id: 178,
    },
    Collation {
        name: "utf32_icelandic_ci",
        charset: "utf32",
        id: 161,
    },
    Collation {
        name: "utf32_latvian_ci",
        charset: "utf32",
        id: 162,
    },
    Collation {
        name: "utf32_lithuanian_ci",
        charset: "utf32",
        id: 172,
    },
    Collation {
        name: "utf32_persian_ci",
        charset: "utf32",
        id: 176,
    },
    Collation {
        name: "utf32_polish_ci",
        charset: "utf32",
        id: 165,
    },
    Collation {
        name: "utf32_roman_ci",
        charset: "utf32",
        id: 175,
    },
    Collation {
        name: "utf32_romanian_ci",
        charset: "utf32",
        id: 163,
    },
    Collation {
        name: "utf32_sinhala_ci",
        charset: "utf32",
        id: 179,
    },
    Collation {
        name: "utf32_slovak_ci",
        charset: "utf32",
        id: 173,
    },
    Collation {
        name: "utf32_slovenian_ci",
        charset: "utf32",
        id: 164,
    },
    Collation {
        name: "utf32_spanish2_ci",
        charset: "utf32",
        id: 174,
    },
    Collation {
        name: "utf32_spanish_ci",
        charset: "utf32",
        id: 167,
    },
    Collation {
        name: "utf32_swedish_ci",
        charset: "utf32",
        id: 168,
    },
    Collation {
        name: "utf32_turkish_ci",
        charset: "utf32",
        id: 169,
    },
    Collation {
        name: "utf32_unicode_520_ci",
        charset: "utf32",
        id: 182,
    },
    Collation {
        name: "utf32_unicode_ci",
        charset: "utf32",
        id: 160,
    },
    Collation {
        name: "utf32_vietnamese_ci",
        charset: "utf32",
        id: 183,
    },
    Collation {
        name: "utf8mb3_bin",
        charset: "utf8mb3",
        id: 83,
    },
    Collation {
        name: "utf8mb3_croatian_ci",
        charset: "utf8mb3",
        id: 213,
    },
    Collation {
        name: "utf8mb3_czech_ci",
        charset: "utf8mb3",
        id: 202,
    },
    Collation {
        name: "utf8mb3_danish_ci",
        charset: "utf8mb3",
        id: 203,
    },
    Collation {
        name: "utf8mb3_esperanto_ci",
        charset: "utf8mb3",
        id: 209,
    },
    Collation {
        name: "utf8mb3_estonian_ci",
        charset: "utf8mb3",
        id: 198,
    },
    Collation {
        name: "utf8mb3_general_ci",
        charset: "utf8mb3",
        id: 33,
    },
    Collation {
        name: "utf8mb3_general_mysql500_ci",
        charset: "utf8mb3",
        id: 223,
    },
    Collation {
        name: "utf8mb3_german2_ci",
        charset: "utf8mb3",
        id: 212,
    },
    Collation {
        name: "utf8mb3_hungarian_ci",
        charset: "utf8mb3",
        id: 210,
    },
    Collation {
        name: "utf8mb3_icelandic_ci",
        charset: "utf8mb3",
        id: 193,
    },
    Collation {
        name: "utf8mb3_latvian_ci",
        charset: "utf8mb3",
        id: 194,
    },
    Collation {
        name: "utf8mb3_lithuanian_ci",
        charset: "utf8mb3",
        id: 204,
    },
    Collation {
        name: "utf8mb3_persian_ci",
        charset: "utf8mb3",
        id: 208,
    },
    Collation {
        name: "utf8mb3_polish_ci",
        charset: "utf8mb3",
        id: 197,
    },
    Collation {
        name: "utf8mb3_roman_ci",
        charset: "utf8mb3",
        id: 207,
    },
    Collation {
        name: "utf8mb3_romanian_ci",
        charset: "utf8mb3",
        id: 195,
    },
    Collation {
        name: "utf8mb3_sinhala_ci",
        charset: "utf8mb3",
        id: 211,
    },
    Collation {
        name: "utf8mb3_slovak_ci",
        charset: "utf8mb3",
        id: 205,
    },
    Collation {
        name: "utf8mb3_slovenian_ci",
        charset: "utf8mb3",
        id: 196,
    },
    Collation {
        name: "utf8mb3_spanish2_ci",
        charset: "utf8mb3",
        id: 206,
    },
    Collation {
        name: "utf8mb3_spanish_ci",
        charset: "utf8mb3",
        id: 199,
    },
    Collation {
        name: "utf8mb3_swedish_ci",
        charset: "utf8mb3",
        id: 200,
    },
    Collation {
        name: "utf8mb3_tolower_ci",
        charset: "utf8mb3",
        id: 76,
    },
    Collation {
        name: "utf8mb3_turkish_ci",
        charset: "utf8mb3",
        id: 201,
    },
    Collation {
        name: "utf8mb3_unicode_520_ci",
        charset: "utf8mb3",
        id: 214,
    },
    Collation {
        name: "utf8mb3_unicode_ci",
        charset: "utf8mb3",
        id: 192,
    },
    Collation {
        name: "utf8mb3_vietnamese_ci",
        charset: "utf8mb3",
        id: 215,
    },
    Collation {
        name: "utf8mb4_0900_ai_ci",
        charset: "utf8mb4",
        id: 255,
    },
    Collation {
        name: "utf8mb4_0900_as_ci",
        charset: "utf8mb4",
        id: 305,
    },
    Collation {
        name: "utf8mb4_0900_as_cs",
        charset: "utf8mb4",
        id: 278,
    },
    Collation {
        name: "utf8mb4_0900_bin",
        charset: "utf8mb4",
        id: 309,
    },
    Collation {
        name: "utf8mb4_bg_0900_ai_ci",
        charset: "utf8mb4",
        id: 318,
    },
    Collation {
        name: "utf8mb4_bg_0900_as_cs",
        charset: "utf8mb4",
        id: 319,
    },
    Collation {
        name: "utf8mb4_bin",
        charset: "utf8mb4",
        id: 46,
    },
    Collation {
        name: "utf8mb4_bs_0900_ai_ci",
        charset: "utf8mb4",
        id: 316,
    },
    Collation {
        name: "utf8mb4_bs_0900_as_cs",
        charset: "utf8mb4",
        id: 317,
    },
    Collation {
        name: "utf8mb4_croatian_ci",
        charset: "utf8mb4",
        id: 245,
    },
    Collation {
        name: "utf8mb4_cs_0900_ai_ci",
        charset: "utf8mb4",
        id: 266,
    },
    Collation {
        name: "utf8mb4_cs_0900_as_cs",
        charset: "utf8mb4",
        id: 289,
    },
    Collation {
        name: "utf8mb4_czech_ci",
        charset: "utf8mb4",
        id: 234,
    },
    Collation {
        name: "utf8mb4_da_0900_ai_ci",
        charset: "utf8mb4",
        id: 267,
    },
    Collation {
        name: "utf8mb4_da_0900_as_cs",
        charset: "utf8mb4",
        id: 290,
    },
    Collation {
        name: "utf8mb4_danish_ci",
        charset: "utf8mb4",
        id: 235,
    },
    Collation {
        name: "utf8mb4_de_pb_0900_ai_ci",
        charset: "utf8mb4",
        id: 256,
    },
    Collation {
        name: "utf8mb4_de_pb_0900_as_cs",
        charset: "utf8mb4",
        id: 279,
    },
    Collation {
        name: "utf8mb4_eo_0900_ai_ci",
        charset: "utf8mb4",
        id: 273,
    },
    Collation {
        name: "utf8mb4_eo_0900_as_cs",
        charset: "utf8mb4",
        id: 296,
    },
    Collation {
        name: "utf8mb4_es_0900_ai_ci",
        charset: "utf8mb4",
        id: 263,
    },
    Collation {
        name: "utf8mb4_es_0900_as_cs",
        charset: "utf8mb4",
        id: 286,
    },
    Collation {
        name: "utf8mb4_es_trad_0900_ai_ci",
        charset: "utf8mb4",
        id: 270,
    },
    Collation {
        name: "utf8mb4_es_trad_0900_as_cs",
        charset: "utf8mb4",
        id: 293,
    },
    Collation {
        name: "utf8mb4_esperanto_ci",
        charset: "utf8mb4",
        id: 241,
    },
    Collation {
        name: "utf8mb4_estonian_ci",
        charset: "utf8mb4",
        id: 230,
    },
    Collation {
        name: "utf8mb4_et_0900_ai_ci",
        charset: "utf8mb4",
        id: 262,
    },
    Collation {
        name: "utf8mb4_et_0900_as_cs",
        charset: "utf8mb4",
        id: 285,
    },
    Collation {
        name: "utf8mb4_general_ci",
        charset: "utf8mb4",
        id: 45,
    },
    Collation {
        name: "utf8mb4_german2_ci",
        charset: "utf8mb4",
        id: 244,
    },
    Collation {
        name: "utf8mb4_gl_0900_ai_ci",
        charset: "utf8mb4",
        id: 320,
    },
    Collation {
        name: "utf8mb4_gl_0900_as_cs",
        charset: "utf8mb4",
        id: 321,
    },
    Collation {
        name: "utf8mb4_hr_0900_ai_ci",
        charset: "utf8mb4",
        id: 275,
    },
    Collation {
        name: "utf8mb4_hr_0900_as_cs",
        charset: "utf8mb4",
        id: 298,
    },
    Collation {
        name: "utf8mb4_hu_0900_ai_ci",
        charset: "utf8mb4",
        id: 274,
    },
    Collation {
        name: "utf8mb4_hu_0900_as_cs",
        charset: "utf8mb4",
        id: 297,
    },
    Collation {
        name: "utf8mb4_hungarian_ci",
        charset: "utf8mb4",
        id: 242,
    },
    Collation {
        name: "utf8mb4_icelandic_ci",
        charset: "utf8mb4",
        id: 225,
    },
    Collation {
        name: "utf8mb4_is_0900_ai_ci",
        charset: "utf8mb4",
        id: 257,
    },
    Collation {
        name: "utf8mb4_is_0900_as_cs",
        charset: "utf8mb4",
        id: 280,
    },
    Collation {
        name: "utf8mb4_ja_0900_as_cs",
        charset: "utf8mb4",
        id: 303,
    },
    Collation {
        name: "utf8mb4_ja_0900_as_cs_ks",
        charset: "utf8mb4",
        id: 304,
    },
    Collation {
        name: "utf8mb4_la_0900_ai_ci",
        charset: "utf8mb4",
        id: 271,
    },
    Collation {
        name: "utf8mb4_la_0900_as_cs",
        charset: "utf8mb4",
        id: 294,
    },
    Collation {
        name: "utf8mb4_latvian_ci",
        charset: "utf8mb4",
        id: 226,
    },
    Collation {
        name: "utf8mb4_lithuanian_ci",
        charset: "utf8mb4",
        id: 236,
    },
    Collation {
        name: "utf8mb4_lt_0900_ai_ci",
        charset: "utf8mb4",
        id: 268,
    },
    Collation {
        name: "utf8mb4_lt_0900_as_cs",
        charset: "utf8mb4",
        id: 291,
    },
    Collation {
        name: "utf8mb4_lv_0900_ai_ci",
        charset: "utf8mb4",
        id: 258,
    },
    Collation {
        name: "utf8mb4_lv_0900_as_cs",
        charset: "utf8mb4",
        id: 281,
    },
    Collation {
        name: "utf8mb4_mn_cyrl_0900_ai_ci",
        charset: "utf8mb4",
        id: 322,
    },
    Collation {
        name: "utf8mb4_mn_cyrl_0900_as_cs",
        charset: "utf8mb4",
        id: 323,
    },
    Collation {
        name: "utf8mb4_nb_0900_ai_ci",
        charset: "utf8mb4",
        id: 310,
    },
    Collation {
        name: "utf8mb4_nb_0900_as_cs",
        charset: "utf8mb4",
        id: 311,
    },
    Collation {
        name: "utf8mb4_nn_0900_ai_ci",
        charset: "utf8mb4",
        id: 312,
    },
    Collation {
        name: "utf8mb4_nn_0900_as_cs",
        charset: "utf8mb4",
        id: 313,
    },
    Collation {
        name: "utf8mb4_persian_ci",
        charset: "utf8mb4",
        id: 240,
    },
    Collation {
        name: "utf8mb4_pl_0900_ai_ci",
        charset: "utf8mb4",
        id: 261,
    },
    Collation {
        name: "utf8mb4_pl_0900_as_cs",
        charset: "utf8mb4",
        id: 284,
    },
    Collation {
        name: "utf8mb4_polish_ci",
        charset: "utf8mb4",
        id: 229,
    },
    Collation {
        name: "utf8mb4_ro_0900_ai_ci",
        charset: "utf8mb4",
        id: 259,
    },
    Collation {
        name: "utf8mb4_ro_0900_as_cs",
        charset: "utf8mb4",
        id: 282,
    },
    Collation {
        name: "utf8mb4_roman_ci",
        charset: "utf8mb4",
        id: 239,
    },
    Collation {
        name: "utf8mb4_romanian_ci",
        charset: "utf8mb4",
        id: 227,
    },
    Collation {
        name: "utf8mb4_ru_0900_ai_ci",
        charset: "utf8mb4",
        id: 306,
    },
    Collation {
        name: "utf8mb4_ru_0900_as_cs",
        charset: "utf8mb4",
        id: 307,
    },
    Collation {
        name: "utf8mb4_sinhala_ci",
        charset: "utf8mb4",
        id: 243,
    },
    Collation {
        name: "utf8mb4_sk_0900_ai_ci",
        charset: "utf8mb4",
        id: 269,
    },
    Collation {
        name: "utf8mb4_sk_0900_as_cs",
        charset: "utf8mb4",
        id: 292,
    },
    Collation {
        name: "utf8mb4_sl_0900_ai_ci",
        charset: "utf8mb4",
        id: 260,
    },
    Collation {
        name: "utf8mb4_sl_0900_as_cs",
        charset: "utf8mb4",
        id: 283,
    },
    Collation {
        name: "utf8mb4_slovak_ci",
        charset: "utf8mb4",
        id: 237,
    },
    Collation {
        name: "utf8mb4_slovenian_ci",
        charset: "utf8mb4",
        id: 228,
    },
    Collation {
        name: "utf8mb4_spanish2_ci",
        charset: "utf8mb4",
        id: 238,
    },
    Collation {
        name: "utf8mb4_spanish_ci",
        charset: "utf8mb4",
        id: 231,
    },
    Collation {
        name: "utf8mb4_sr_latn_0900_ai_ci",
        charset: "utf8mb4",
        id: 314,
    },
    Collation {
        name: "utf8mb4_sr_latn_0900_as_cs",
        charset: "utf8mb4",
        id: 315,
    },
    Collation {
        name: "utf8mb4_sv_0900_ai_ci",
        charset: "utf8mb4",
        id: 264,
    },
    Collation {
        name: "utf8mb4_sv_0900_as_cs",
        charset: "utf8mb4",
        id: 287,
    },
    Collation {
        name: "utf8mb4_swedish_ci",
        charset: "utf8mb4",
        id: 232,
    },
    Collation {
        name: "utf8mb4_tr_0900_ai_ci",
        charset: "utf8mb4",
        id: 265,
    },
    Collation {
        name: "utf8mb4_tr_0900_as_cs",
        charset: "utf8mb4",
        id: 288,
    },
    Collation {
        name: "utf8mb4_turkish_ci",
        charset: "utf8mb4",
        id: 233,
    },
    Collation {
        name: "utf8mb4_unicode_520_ci",
        charset: "utf8mb4",
        id: 246,
    },
    Collation {
        name: "utf8mb4_unicode_ci",
        charset: "utf8mb4",
        id: 224,
    },
    Collation {
        name: "utf8mb4_vi_0900_ai_ci",
        charset: "utf8mb4",
        id: 277,
    },
    Collation {
        name: "utf8mb4_vi_0900_as_cs",
        charset: "utf8mb4",
        id: 300,
    },
    Collation {
        name: "utf8mb4_vietnamese_ci",
        charset: "utf8mb4",
        id: 247,
    },
    Collation {
        name: "utf8mb4_zh_0900_as_cs",
        charset: "utf8mb4",
        id: 308,
    },
];
