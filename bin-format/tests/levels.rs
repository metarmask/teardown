macro_rules! test_file {
    ($($file:ident)*) => {
        $(
            #[test]
            fn $file() {
                teardown_bin_format::parse_file("../example-input/bin/".to_owned() + stringify!($file) + ".bin").unwrap();
            }
        )*
    };
}

test_file! {about carib_alarm carib_barrels carib_destroy carib_sandbox carib_yacht caveisland_computers caveisland_dishes caveisland_ingredients caveisland_propane caveisland_roboclear caveisland_sandbox ch_carib_fetch ch_carib_hunted ch_carib_mayhem ch_caveisland_fetch ch_caveisland_hunted ch_caveisland_mayhem ch_factory_fetch ch_factory_hunted ch_factory_mayhem ch_frustrum_fetch ch_frustrum_hunted ch_frustrum_mayhem ch_lee_fetch ch_lee_hunted ch_lee_mayhem ch_mall_fetch ch_mall_hunted ch_mall_mayhem ch_mansion_fetch ch_mansion_hunted ch_mansion_mayhem ch_marina_fetch ch_marina_hunted ch_marina_mayhem cullington_bomb ending10 ending20 ending21 ending22 factory_espionage factory_explosive factory_robot factory_sandbox factory_tools frustrum_chase frustrum_pawnshop frustrum_sandbox frustrum_tornado frustrum_vehicle hub0 hub1 hub2 hub3 hub4 hub5 hub6 hub7 hub8 hub9 hub10 hub11 hub12 hub13 hub14 hub15 hub16 hub20 hub21 hub22 hub23 hub24 hub30 hub31 hub32 hub33 hub34 hub40 hub41 hub42 hub43 hub44 hub45 hub46 lee_computers lee_flooding lee_login lee_powerplant lee_safe lee_sandbox lee_tower lee_woonderland mall_decorations mall_foodcourt mall_intro mall_radiolink mall_sandbox mall_shipping mansion_art mansion_fraud mansion_pool mansion_race mansion_safe mansion_sandbox marina_art_back marina_cars marina_demolish marina_gps marina_sandbox marina_tools}
