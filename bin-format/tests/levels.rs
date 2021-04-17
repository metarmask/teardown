macro_rules! test_file {
    ($($file:ident)*) => {
        $(
            #[test]
            fn $file() {
                // To see tests that fail when they allocate too much.
                println!("doing {}", stringify!($file));
                teardown_bin_format::parse_file("../example-input/bin/".to_owned() + stringify!($file) + ".bin").unwrap();
            }
        )*
    };
}

test_file! {about caveisland_computers caveisland_dishes caveisland_propane caveisland_sandbox ch_caveisland_hunted ch_caveisland_mayhem ch_frustrum_hunted ch_frustrum_mayhem ch_lee_hunted ch_lee_mayhem ch_mansion_hunted ch_mansion_mayhem ch_marina_hunted ch_marina_mayhem frustrum_chase frustrum_sandbox hub0 hub10 hub11 hub12 hub13 hub14 hub15 hub16 hub1 hub2 hub3 hub4 hub5 hub6 hub7 hub8 hub9 lee_computers lee_flooding lee_login lee_powerplant lee_safe lee_sandbox lee_tower mall_intro mansion_art mansion_fraud mansion_pool mansion_race mansion_safe mansion_sandbox marina_art_back marina_cars marina_demolish marina_gps marina_sandbox marina_tools}
