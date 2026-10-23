//! Conversion from raw vanilla effect IDs to `EffectType`.

use zerocopy::TryFromBytes;

use super::EffectType;

impl EffectType {
    /// Try to convert from a raw u16 value.
    /// Returns None for obsolete or unknown values.
    #[must_use]
    pub fn from_u16(value: u16) -> Option<Self> {
        if value == Self::SetTeleporterDestination as u16 {
            return Some(Self::SetTeleporterDestination);
        }
        if Self::is_known_early_type(value) || Self::is_known_late_type(value) {
            return Self::try_read_from_bytes(&value.to_ne_bytes()).ok();
        }
        None
    }

    fn is_known_early_type(value: u16) -> bool {
        matches!(
            value,
            31..=38
            | 50..=53
            | 55..=61
            | 65
            | 66
            | 69
            | 71
            | 74
            | 75
            | 77..=79
            | 83..=103
            | 110..=113
            | 117
            | 120..=127
            | 130
            | 132
            | 133
            | 135
            | 137
            | 142..=154
            | 159..=164
            | 167..=175
            | 178..=181
            | 183..=190
            | 193
            | 207 | 209 | 218 | 221
            | 230
            | 237
            | 239
            | 243
            | 255
            | 257..=263
            | 265
            | 267
            | 277..=279
            | 281
            | 283..=298
            | 299
            | 300..=306
            | 308
            | 309
            | 311
            | 312
            | 316..=320
            | 324
            | 325
            | 330
            | 334..=351
            | 353..=365
            | 369..=379
            | 382
            | 385
            | 388..=393
        )
    }

    fn is_known_late_type(value: u16) -> bool {
        matches!(
            value,
            | 413
            | 417..=419
            | 425
            | 430..=433
            | 439
            | 443
            | 446..=451
            | 455..=460
            | 463
            | 471
            | 472
            | 475
            | 480..=484
            | 489..=493
            | 498
            | 499..=501
            | 510
            | 516
            | 519
            | 520
            | 525
            | 526
            | 532
            | 554
            | 559
            | 562
            | 569..=574
            | 580..=582
            | 588
            | 609
            | 630
            | 632
            | 633
            | 634
            | 646 | 647
            | 658 | 659
            | 668 | 669 | 672 | 674
            | 670
            | 711
            | 720
            | 678 | 679
            | 687
             | 712
             | 713
            | 716..=719
            | 729
            | 736
            | 741
            | 748
            | 773 | 774
            | 781
            | 809
            | 810
            | 811
            | 812
            | 818
            | 833
            | 836
            | 837..=840
            | 841
            | 852
            | 863
            | 867..=870
            | 872
            | 875
            | 884
            | 900..=902
            | 904
            | 912
            | 914 | 915
            | 921 | 922
            | 935..=938
            | 940
            | 943
            | 970
            | 984
            | 988
            | 989
            | 1000
            | 1001
            | 1007
            | 1018
            | 1034
            | 1037
            | 1044
            | 1045
            | 1048
            | 1054
            | 1061
            | 1065
            // Campaign end and UI presentation without user input.
            | 515 | 521 | 533 | 726 | 775 | 805 | 808 | 815 | 923 | 1002 | 1006 | 1030 | 1049
        )
    }
}
