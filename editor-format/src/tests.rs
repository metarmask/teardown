#[allow(clippy::approx_constant, clippy::unreadable_literal)]
mod transform_shape {
    use approx::assert_relative_eq;

    use super::super::*;

    fn rot(x: f32, y: f32, z: f32) -> [f32; 4] {
        let quat =
            UnitQuaternion::from_euler_angles(x.to_radians(), y.to_radians(), z.to_radians());
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
