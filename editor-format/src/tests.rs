#![allow(clippy::approx_constant, clippy::unreadable_literal, clippy::manual_assert, clippy::semicolon_if_nothing_returned)]
mod transform_shape {
    use approx::assert_relative_eq;
    use nalgebra::{UnitQuaternion, Vector3};
    use teardown_bin_format::Transform;

    use crate::vox::transform_shape;

    fn rot(x: f32, y: f32, z: f32) -> [f32; 4] {
        let x = UnitQuaternion::from_axis_angle(&Vector3::x_axis(), x.to_radians());
        let y = UnitQuaternion::from_axis_angle(&Vector3::y_axis(), y.to_radians());
        let z = UnitQuaternion::from_axis_angle(&Vector3::z_axis(), z.to_radians());
        // YZX order
        let quat = (y * z) * x;
        [quat.i, quat.j, quat.k, quat.w]
    }

    #[test]
    fn at_origin_no_rotation() {
        assert_relative_eq!(
            transform_shape(
                &Transform {
                    pos: [-0.5, 0.0, 0.5],
                    rot: [-0.7071068, 0.0, 0.0, 0.7071068]
                },
                [10, 10, 10]
            ),
            Transform {
                pos: [0., 0., 0.],
                rot: rot(0., 0., 0.)
            }
        )
    }

    #[test]
    fn at_origin_45_x() {
        assert_relative_eq!(
            transform_shape(
                &Transform {
                    pos: [-0.5, -0.3535534, 0.35355335],
                    rot: [-0.38268343, 0.0, 0.0, 0.92387956]
                },
                [10, 10, 10]
            ),
            Transform {
                pos: [0., 0., 0.],
                rot: rot(45., 0., 0.)
            }
        )
    }

    #[test]
    fn at_origin_45_y() {
        assert_relative_eq!(
            transform_shape(
                &Transform {
                    pos: [0.000000059604645, 0.0, 0.70710677],
                    rot: [-0.6532815, 0.27059808, 0.27059808, 0.6532815]
                },
                [10, 10, 10]
            ),
            Transform {
                pos: [0., 0., 0.],
                rot: rot(0., 45., 0.)
            }
        )
    }

    #[test]
    fn at_origin_90_y() {
        assert_relative_eq!(
            transform_shape(
                &Transform {
                    pos: [0.5, 0.0, 0.49999994],
                    rot: [-0.5, 0.5, 0.5, 0.5]
                },
                [10, 10, 10]
            ),
            Transform {
                pos: [0., 0., 0.],
                rot: rot(0., 90., 0.)
            }
        )
    }

    #[test]
    fn at_origin_20_z() {
        assert_relative_eq!(
            transform_shape(
                &Transform {
                    pos: [-0.4698462, -0.1710101, 0.4999998],
                    rot: [-0.6963643, -0.12278781, 0.12278781, 0.6963643]
                },
                [10, 10, 10]
            ),
            Transform {
                pos: [0., 0., 0.],
                rot: rot(0., 0., 20.)
            }
        )
    }

    #[test]
    fn at_origin_45_45_45() {
        assert_relative_eq!(
            transform_shape(
                &Transform {
                    pos: [0.17677675, -0.60355335, 0.32322317],
                    rot: [-0.19134167, 0.19134174, 0.46193975, 0.8446232]
                },
                [10, 10, 10]
            ),
            Transform {
                pos: [0., 0., 0.],
                rot: rot(45., 45., 45.)
            }
        )
    }

    #[test]
    fn positive_x() {
        assert_relative_eq!(
            transform_shape(
                &Transform {
                    pos: [1.5, 0.0, 0.5],
                    rot: [-0.7071068, 0.0, 0.0, 0.7071068]
                },
                [10, 10, 10]
            ),
            Transform {
                pos: [2.0, 0.0, 0.0],
                rot: rot(0., 0., 0.)
            }
        )
    }

    #[test]
    fn negative_x() {
        assert_relative_eq!(
            transform_shape(
                &Transform {
                    pos: [-2.5, 0.0, 0.5],
                    rot: [-0.7071068, 0.0, 0.0, 0.7071068]
                },
                [10, 10, 10]
            ),
            Transform {
                pos: [-2.0, 0.0, 0.0],
                rot: rot(0., 0., 0.)
            }
        )
    }

    #[test]
    fn odd_z() {
        assert_relative_eq!(
            transform_shape(
                &Transform {
                    pos: [-0.5, 0.0, 1.0],
                    rot: [-0.7071068, 0.0, 0.0, 0.7071068]
                },
                [10, 1, 1]
            ),
            Transform {
                pos: [0.0, 0.0, 1.0],
                rot: rot(0., 0., 0.)
            }
        )
    }

    #[test]
    fn odd_negative_z() {
        assert_relative_eq!(
            transform_shape(
                &Transform {
                    pos: [-0.5, 0.0, -1.0],
                    rot: [-0.7071068, 0.0, 0.0, 0.7071068]
                },
                [10, 1, 1]
            ),
            Transform {
                pos: [0.0, 0.0, -1.0],
                rot: rot(0., 0., 0.)
            }
        )
    }

    #[test]
    fn odd_at_origin() {
        assert_relative_eq!(
            transform_shape(
                &Transform {
                    pos: [-0.4, 0.0, 0.1],
                    rot: [-0.7071068, 0.0, 0.0, 0.7071068]
                },
                [9, 3, 7]
            ),
            Transform {
                pos: [0.0, 0.0, 0.0],
                rot: rot(0., 0., 0.)
            }
        )
    }

    #[test]
    fn origin_xy_45() {
        assert_relative_eq!(
            transform_shape(
                &Transform {
                    pos: [-0.10355337, -0.3535534, 0.6035534],
                    rot: [-0.35355338, 0.35355344, 0.1464466, 0.8535534]
                },
                [10, 10, 10]
            ),
            Transform {
                pos: [0.0, 0.0, 0.0],
                rot: rot(45., 45., 0.)
            }
        )
    }
}

mod palette {
    use teardown_bin_format::{Material, MaterialKind};

    use crate::{
        util::IntoFixedArray,
        vox::{remap_materials, PaletteMapping},
    };

    #[test]
    fn preserve_original() {
        let mut materials: [Material; 256] = vec![Material::default(); 256].into_fixed();
        materials[4] = Material {
            replacable: false,
            kind: MaterialKind::Glass,
            ..Material::default()
        };
        assert!(matches!(
            remap_materials(&materials),
            PaletteMapping::Original(_)
        ));
    }

    #[test]
    fn remap_non_replacable() {
        let mut materials: [Material; 256] = vec![Material::default(); 256].into_fixed();
        let kind = MaterialKind::Dirt;
        materials[4] = Material {
            replacable: false,
            kind,
            ..Material::default()
        };
        if let PaletteMapping::Remapped(boxed) = remap_materials(&materials) {
            let (remapped, indices_orig_to_new) = boxed.as_ref();
            assert_eq!(remapped[indices_orig_to_new[4] as usize].kind, kind);
            assert_ne!(remapped[4].kind, kind);
        } else {
            panic!("should be remapped");
        }
    }

    #[test]
    fn replace_replacables() {
        let mut materials: [Material; 256] = vec![Material::default(); 256].into_fixed();
        #[allow(clippy::needless_range_loop)]
        for i in 169..=176 {
            materials[i] = Material {
                replacable: true,
                kind: MaterialKind::HardMetal,
                ..Material::default()
            };
        }
        materials[1] = Material {
            replacable: false,
            kind: MaterialKind::HardMetal,
            ..Material::default()
        };
        if let PaletteMapping::Remapped(boxed) = remap_materials(&materials) {
            let (remapped, indices_orig_to_new) = boxed.as_ref();
            assert!(!remapped[indices_orig_to_new[1] as usize].replacable);
        } else {
            panic!("should be remapped");
        }
    }

    #[test]
    fn keep_brake_light_index() {
        let mut materials: [Material; 256] = vec![Material::default(); 256].into_fixed();
        materials[6] = Material {
            replacable: false,
            kind: MaterialKind::Glass,
            ..Material::default()
        };
        materials[2] = Material {
            replacable: false,
            kind: MaterialKind::Dirt,
            ..Material::default()
        };
        assert_eq!(
            remap_materials(&materials).materials_as_ref()[6].kind,
            MaterialKind::Glass
        );
    }
}

mod convert_material {
    use ::vox::semantic::{Material as VoxMaterial, MaterialKind as VoxMaterialKind};
    use approx::assert_relative_eq;
    use teardown_bin_format::{Material, Rgba};

    use crate::vox::convert_material;

    #[test]
    fn re00_s10_m00() {
        let vox_mat = convert_material(&Material {
            shinyness: 1.0,
            ..Material::default()
        });
        assert_eq!(vox_mat.kind, VoxMaterialKind::Metal);
        assert_relative_eq!(vox_mat.metal.unwrap_or_default(), 0.);
        assert_relative_eq!(vox_mat.rough.unwrap_or_default(), 0.);
    }

    #[test]
    fn re05_s00_m00() {
        let vox_mat = convert_material(&Material {
            shinyness: 0.5,
            ..Material::default()
        });
        assert_eq!(vox_mat.kind, VoxMaterialKind::Metal);
        assert_relative_eq!(vox_mat.metal.unwrap_or_default(), 0.);
        assert_relative_eq!(vox_mat.rough.unwrap_or_default(), 0.5);
    }

    #[test]
    fn re00_s00_m00() {
        let vox_mat = convert_material(&Material::default());
        assert_eq!(vox_mat.kind, VoxMaterialKind::Metal);
        assert_relative_eq!(vox_mat.metal.unwrap_or_default(), 0.);
        assert_relative_eq!(vox_mat.rough.unwrap_or_default(), 1.);
    }

    #[test]
    fn re05_s10_m05() {
        let vox_mat = convert_material(&Material {
            reflectivity: 0.5,
            shinyness: 1.0,
            metalness: 0.5,
            ..Material::default()
        });
        assert_eq!(vox_mat.kind, VoxMaterialKind::Metal);
        assert_relative_eq!(vox_mat.metal.unwrap_or_default(), 0.5);
        assert_relative_eq!(vox_mat.rough.unwrap_or_default(), 0.);
    }

    #[test]
    fn re05_s05_m05() {
        let vox_mat = convert_material(&Material {
            reflectivity: 0.5,
            shinyness: 0.5,
            metalness: 0.5,
            ..Material::default()
        });
        assert_eq!(vox_mat.kind, VoxMaterialKind::Metal);
        assert_relative_eq!(vox_mat.metal.unwrap_or_default(), 0.5);
        assert_relative_eq!(vox_mat.rough.unwrap_or_default(), 0.5);
    }

    #[test]
    fn re05_s00_m05() {
        let vox_mat = convert_material(&Material {
            reflectivity: 0.5,
            shinyness: 0.0,
            metalness: 0.5,
            ..Material::default()
        });
        assert_eq!(vox_mat.kind, VoxMaterialKind::Metal);
        assert_relative_eq!(vox_mat.metal.unwrap_or_default(), 0.5);
        assert_relative_eq!(vox_mat.rough.unwrap_or_default(), 1.0);
    }

    #[test]
    fn re10_s10_m10() {
        let vox_mat = convert_material(&Material {
            reflectivity: 1.0,
            shinyness: 1.0,
            metalness: 1.0,
            ..Material::default()
        });
        assert_eq!(vox_mat.kind, VoxMaterialKind::Metal);
        assert_relative_eq!(vox_mat.metal.unwrap_or_default(), 1.0);
        assert_relative_eq!(vox_mat.rough.unwrap_or_default(), 0.0);
    }

    #[test]
    fn re10_s05_m10() {
        let vox_mat = convert_material(&Material {
            reflectivity: 1.0,
            shinyness: 0.5,
            metalness: 1.0,
            ..Material::default()
        });
        assert_eq!(vox_mat.kind, VoxMaterialKind::Metal);
        assert_relative_eq!(vox_mat.metal.unwrap_or_default(), 1.0);
        assert_relative_eq!(vox_mat.rough.unwrap_or_default(), 0.5);
    }

    #[test]
    fn s100_m0_re100_e0() {
        let vox_mat = convert_material(&Material {
            reflectivity: 1.0,
            shinyness: 0.0,
            metalness: 1.0,
            ..Material::default()
        });
        assert_eq!(vox_mat.kind, VoxMaterialKind::Metal);
        assert_relative_eq!(vox_mat.metal.unwrap_or_default(), 1.0);
        assert_relative_eq!(vox_mat.rough.unwrap_or_default(), 1.0);
    }

    fn vox_mat_emission(vox_mat: &VoxMaterial) -> f32 {
        let emit = vox_mat.emit.unwrap_or_default();
        assert!(emit <= 1.0);
        emit * 10_f32.powf(vox_mat.flux.unwrap_or_default() - 1.0)
    }

    #[test]
    fn emission_100() {
        let vox_mat = convert_material(&Material {
            emission: 100.,
            ..Material::default()
        });
        assert_eq!(vox_mat.kind, VoxMaterialKind::Emit);
        assert_relative_eq!(vox_mat_emission(&vox_mat), 100.);
    }

    #[test]
    fn emission_50() {
        let vox_mat = convert_material(&Material {
            emission: 50.,
            ..Material::default()
        });
        assert_eq!(vox_mat.kind, VoxMaterialKind::Emit);
        assert_relative_eq!(vox_mat_emission(&vox_mat), 50.);
    }

    #[test]
    fn emission_05() {
        let vox_mat = convert_material(&Material {
            emission: 0.5,
            ..Material::default()
        });
        assert_eq!(vox_mat.kind, VoxMaterialKind::Emit);
        assert_relative_eq!(vox_mat_emission(&vox_mat), 0.5);
    }

    #[test]
    fn alpha_50() {
        let vox_mat = convert_material(&Material {
            rgba: Rgba([0., 0., 0., 0.5]),
            ..Material::default()
        });
        assert_eq!(vox_mat.kind, VoxMaterialKind::Glass);
        assert!(vox_mat.alpha.unwrap_or_default() < 1.0);
    }
}
