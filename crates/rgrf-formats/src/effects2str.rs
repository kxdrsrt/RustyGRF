//! Effect number to the STR file it plays: the table of GRF Editor's `Effects2Str.cs` (with
//! permission, see `docs/05-licensing.md`).
//!
//! The original stores the Korean names as Latin-1 text (mojibake); here they are the real names.
//! Entry 134 is `freezed.str`; the original assigns it a second time with an empty name at the end,
//! which we do not copy.

/// Effect number and STR file name, sorted by number.
pub const EFFECTS: &[(i32, &str)] = &[
    (10, "maemor.str"),
    (13, "safetywall.str"),
    (23, "stonecurse.str"),
    (25, "firewall2.str"),
    (29, "lightning.str"),
    (30, "thunderstorm.str"),
    (40, "cross.str"),
    (41, "angelus.str"),
    (49, "firehit2.str"),
    (52, "windhit3.str"),
    (64, "arrowshot.str"),
    (65, "invenom.str"),
    (66, "cure.str"),
    (67, "provoke.str"),
    (68, "mvp.str"),
    (69, "skidtrap.str"),
    (70, "brandish.str"),
    (75, "gloria.str"),
    (76, "magnificat.str"),
    (77, "resurrection.str"),
    (78, "recovery.str"),
    (83, "sanctuary.str"),
    (84, "impositio.str"),
    (85, "lexaeterna.str"),
    (86, "aspersio.str"),
    (87, "lexdivina.str"),
    (88, "suffragium.str"),
    (89, "stormgust.str"),
    (90, "lord.str"),
    (91, "benedictio.str"),
    (92, "meteor3.str"),
    (95, "quadmire.str"),
    (96, "firepillar.str"),
    (97, "firepillarbomb.str"),
    (101, "repairweapon.str"),
    (102, "crashearth.str"),
    (103, "weaponperfection.str"),
    (104, "maximizepower.str"),
    (106, "blastmine.str"),
    (107, "claymore.str"),
    (108, "freezing.str"),
    (109, "bubble1.str"),
    (110, "gaspush.str"),
    (111, "spring.str"),
    (112, "kyrie.str"),
    (113, "magnus.str"),
    (124, "venomdust.str"),
    (126, "poisonreact_1st.str"),
    (127, "poisonreact.str"),
    (129, "venomsplasher.str"),
    (130, "twohand.str"),
    (131, "autocounter.str"),
    (133, "freeze.str"),
    (134, "freezed.str"),
    (135, "icecrash.str"),
    (136, "slowp.str"),
    (139, "sandman.str"),
    (141, "pneuma1.str"),
    (143, "sonicblow.str"),
    (144, "brandish2.str"),
    (145, "shockwave.str"),
    (146, "shockwavehit.str"),
    (147, "earthhit.str"),
    (148, "pierce.str"),
    (149, "bowling.str"),
    (150, "spearstab.str"),
    (151, "spearboomerang.str"),
    (152, "holyhit.str"),
    (153, "concentration.str"),
    (154, "bs_refinesuccess.str"),
    (155, "bs_refinefailed.str"),
    (158, "joblvup.str"),
    (169, "energycoat.str"),
    (170, "cartrevolution.str"),
    (181, "mentalbreak.str"),
    (182, "magical.str"),
    (183, "sui_explosion.str"),
    (185, "suicide.str"),
    (186, "yunta_1.str"),
    (187, "yunta_2.str"),
    (188, "yunta_3.str"),
    (189, "yunta_4.str"),
    (190, "yunta_5.str"),
    (191, "homing.str"),
    (192, "poison.str"),
    (193, "silence.str"),
    (194, "stun.str"),
    (195, "stonecurse.str"),
    (197, "sleep.str"),
    (199, "pong1.str"),
    (204, "빨간포션.str"),
    (205, "주홍포션.str"),
    (206, "노란포션.str"),
    (207, "하얀포션.str"),
    (208, "파란포션.str"),
    (209, "초록포션.str"),
    (210, "fruit.str"),
    (211, "fruit_.str"),
    (212, "darkbreath.str"),
    (213, "deffender.str"),
    (214, "keeping.str"),
    (218, "집중.str"),
    (219, "각성.str"),
    (220, "버서크.str"),
    (234, "spell.str"),
    (235, "디스펠.str"),
    (244, "매직로드.str"),
    (245, "holy_cross.str"),
    (246, "shield_charge.str"),
    (248, "providence.str"),
    (249, "twohand.str"),
    (251, "devotion.str"),
    (255, "enc_fire.str"),
    (256, "enc_ice.str"),
    (257, "enc_wind.str"),
    (258, "enc_earth.str"),
    (268, "steal_coin.str"),
    (269, "strip_weapon.str"),
];

/// The STR file of effect `id`.
pub fn str_of(id: i32) -> Option<&'static str> {
    EFFECTS
        .binary_search_by_key(&id, |&(i, _)| i)
        .ok()
        .map(|p| EFFECTS[p].1)
}

/// The effect numbers that play the STR file `name` (a file name or a path, any case).
pub fn ids_of(name: &str) -> Vec<i32> {
    let file = name.rsplit(['\\', '/']).next().unwrap_or(name);
    EFFECTS
        .iter()
        .filter(|(_, n)| n.eq_ignore_ascii_case(file))
        .map(|&(i, _)| i)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_and_names_find_each_other() {
        assert_eq!(str_of(30), Some("thunderstorm.str"));
        assert_eq!(str_of(134), Some("freezed.str"));
        assert_eq!(str_of(204), Some("빨간포션.str"));
        assert_eq!(str_of(1), None);
        assert_eq!(ids_of("data\\texture\\effect\\Stonecurse.STR"), [23, 195]);
        assert!(ids_of("nothing.str").is_empty());
        assert!(
            EFFECTS.windows(2).all(|w| w[0].0 < w[1].0),
            "sorted for the search"
        );
        assert_eq!(EFFECTS.len(), 118);
    }
}
