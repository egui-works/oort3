use crate::rng;
use crate::ship::{ShipClass, ShipHandle};
use crate::simulation::{Line, Simulation};
use nalgebra::Rotation2;
use nalgebra::{vector, Point2, Vector2};
use rand::Rng;
use rand_distr::StandardNormal;
use rng::SeededRng;
use std::f64::consts::TAU;

#[derive(Clone, Debug)]
pub struct Radar {
    pub heading: f64,
    pub width: f64,
    pub power: f64,
    pub rx_cross_section: f64,
    pub min_rssi: f64,
    pub classify_rssi: f64,
    pub result: Option<ScanResult>,
}

struct RadarEmitter {
    handle: ShipHandle,
    center: Point2<f64>,
    width: f64,
    start_bearing: f64,
    end_bearing: f64,
    power: f64,
    rx_cross_section: f64,
    min_rssi: f64,
    classify_rssi: f64,
    team: i32,
}

struct RadarReflector {
    position: Point2<f64>,
    velocity: Vector2<f64>,
    radar_cross_section: f64,
    team: i32,
    class: ShipClass,
}

#[derive(Copy, Clone, Debug)]
pub struct ScanResult {
    pub class: Option<ShipClass>,
    pub position: Vector2<f64>,
    pub velocity: Vector2<f64>,
}

pub fn scan(sim: &mut Simulation, own_ship: ShipHandle) -> Option<ScanResult> {
    if let Some(radar) = sim.ship(own_ship).data().radar.as_ref() {
        radar.result
    } else {
        None
    }
}

#[inline(never)]
pub fn tick(sim: &mut Simulation) {
    let handle_snapshot: Vec<ShipHandle> = sim.ships.iter().cloned().collect();

    let reflectors: Vec<RadarReflector> = handle_snapshot
        .iter()
        .cloned()
        .map(|handle| {
            let ship = sim.ship(handle);
            let ship_data = ship.data();
            RadarReflector {
                team: ship_data.team,
                position: ship.position().vector.into(),
                velocity: ship.velocity(),
                radar_cross_section: ship_data.radar_cross_section,
                class: ship_data.class,
            }
        })
        .collect();

    for handle in handle_snapshot.iter().cloned() {
        let ship = sim.ship(handle);
        let ship_data = ship.data();

        if let Some(radar) = ship_data.radar.as_ref() {
            let h = radar.heading + ship.heading();
            let w = radar.width;
            let emitter = RadarEmitter {
                handle,
                team: ship_data.team,
                center: ship.position().vector.into(),
                power: radar.power,
                min_rssi: radar.min_rssi,
                classify_rssi: radar.classify_rssi,
                rx_cross_section: radar.rx_cross_section,
                width: w,
                start_bearing: h - 0.5 * w,
                end_bearing: h + 0.5 * w,
            };
            let mut rng = rng::new_rng(sim.tick());

            let mut best_rssi = emitter.min_rssi;
            let mut best_reflector: Option<&RadarReflector> = None;
            for reflector in &reflectors {
                if emitter.team == reflector.team {
                    continue;
                }

                if !check_inside_beam(&emitter, &reflector.position) {
                    continue;
                }

                let rssi = compute_rssi(&emitter, reflector);
                if rssi > best_rssi {
                    best_reflector = Some(reflector);
                    best_rssi = rssi;
                }
            }

            let result = best_reflector.map(|reflector| ScanResult {
                class: if best_rssi > emitter.classify_rssi {
                    Some(reflector.class)
                } else {
                    None
                },
                position: reflector.position.coords + noise(&mut rng, best_rssi),
                velocity: reflector.velocity + noise(&mut rng, best_rssi),
            });

            sim.ship_mut(emitter.handle)
                .data_mut()
                .radar
                .as_mut()
                .unwrap()
                .result = result;
            draw_emitter(sim, &emitter);
        }
    }
}

fn check_inside_beam(emitter: &RadarEmitter, point: &Point2<f64>) -> bool {
    if emitter.width >= TAU {
        return true;
    }
    let ray0 = Rotation2::new(emitter.start_bearing).transform_vector(&vector![1.0, 0.0]);
    let ray1 = Rotation2::new(emitter.end_bearing).transform_vector(&vector![1.0, 0.0]);
    let dp = point - emitter.center;
    let is_clockwise = |v0: Vector2<f64>, v1: Vector2<f64>| -v0.x * v1.y + v0.y * v1.x > 0.0;
    if is_clockwise(ray1, ray0) {
        !is_clockwise(ray0, dp) && is_clockwise(ray1, dp)
    } else {
        is_clockwise(ray1, dp) || !is_clockwise(ray0, dp)
    }
}

fn compute_rssi(emitter: &RadarEmitter, reflector: &RadarReflector) -> f64 {
    let r_sq = nalgebra::distance_squared(&emitter.center, &reflector.position);
    emitter.power * reflector.radar_cross_section * emitter.rx_cross_section
        / (TAU * emitter.width * r_sq)
}

fn compute_approx_range(emitter: &RadarEmitter) -> f64 {
    let target_cross_section = 5.0;
    (emitter.power * target_cross_section * emitter.rx_cross_section
        / (TAU * emitter.width * emitter.min_rssi))
        .sqrt()
}

fn noise(rng: &mut SeededRng, rssi: f64) -> Vector2<f64> {
    vector![rng.sample(StandardNormal), rng.sample(StandardNormal)] * (1.0 / rssi)
}

fn draw_emitter(sim: &mut Simulation, emitter: &RadarEmitter) {
    let color = vector![0.1, 0.2, 0.3, 1.0];
    let mut lines = vec![];
    let n = 20;
    let w = emitter.end_bearing - emitter.start_bearing;
    let center = emitter.center;
    let r = compute_approx_range(emitter);
    for i in 0..n {
        let frac = (i as f64) / (n as f64);
        let angle_a = emitter.start_bearing + w * frac;
        let angle_b = emitter.start_bearing + w * (frac + 1.0 / n as f64);
        lines.push(Line {
            a: center + vector![r * angle_a.cos(), r * angle_a.sin()],
            b: center + vector![r * angle_b.cos(), r * angle_b.sin()],
            color,
        });
    }
    lines.push(Line {
        a: center,
        b: center
            + vector![
                r * emitter.start_bearing.cos(),
                r * emitter.start_bearing.sin()
            ],
        color,
    });
    lines.push(Line {
        a: center,
        b: center + vector![r * emitter.end_bearing.cos(), r * emitter.end_bearing.sin()],
        color,
    });
    sim.emit_debug_lines(emitter.handle, &lines);
}

#[cfg(test)]
mod test {
    use crate::ship;
    use crate::simulation::Code;
    use crate::simulation::Simulation;
    use nalgebra::{vector, UnitComplex};
    use rand::Rng;
    use std::f64::consts::TAU;
    use test_log::test;

    const EPSILON: f64 = 0.01;

    #[test]
    fn test_basic() {
        let mut sim = Simulation::new("test", 0, &Code::None);

        // Initial state.
        let ship0 = ship::create(&mut sim, 0.0, 0.0, 0.0, 0.0, 0.0, ship::fighter(0));
        let ship1 = ship::create(&mut sim, 1000.0, 0.0, 0.0, 0.0, 0.0, ship::target(1));
        sim.step();
        assert_eq!(sim.ship(ship0).radar().unwrap().result.is_some(), true);

        // Explicit heading and width.
        sim.ship_mut(ship0).radar_mut().unwrap().heading = 0.0;
        sim.ship_mut(ship0).radar_mut().unwrap().width = TAU / 6.0;
        sim.step();
        assert_eq!(sim.ship(ship0).radar().unwrap().result.is_some(), true);

        // Just outside of sector (clockwise).
        sim.ship_mut(ship0).radar_mut().unwrap().heading = TAU / 12.0 + EPSILON;
        sim.ship_mut(ship0).radar_mut().unwrap().width = TAU / 6.0;
        sim.step();
        assert_eq!(sim.ship(ship0).radar().unwrap().result.is_some(), false);

        // Just inside of sector (clockwise).
        sim.ship_mut(ship0).radar_mut().unwrap().heading -= 2.0 * EPSILON;
        sim.step();
        assert_eq!(sim.ship(ship0).radar().unwrap().result.is_some(), true);

        // Just outside of sector (counter-clockwise).
        sim.ship_mut(ship0).radar_mut().unwrap().heading = -TAU / 12.0 - EPSILON;
        sim.ship_mut(ship0).radar_mut().unwrap().width = TAU / 6.0;
        sim.step();
        assert_eq!(sim.ship(ship0).radar().unwrap().result.is_some(), false);

        // Just inside of sector (counter-clockwise).
        sim.ship_mut(ship0).radar_mut().unwrap().heading += 2.0 * EPSILON;
        sim.step();
        assert_eq!(sim.ship(ship0).radar().unwrap().result.is_some(), true);

        // Out of range.
        sim.ship_mut(ship0).radar_mut().unwrap().heading = 0.0;
        sim.ship_mut(ship0).radar_mut().unwrap().width = TAU / 6.0;
        sim.ship_mut(ship1)
            .body()
            .set_translation(vector![1e6, 0.0], true);
        sim.step();
        assert_eq!(sim.ship(ship0).radar().unwrap().result.is_some(), false);
    }

    #[test]
    fn test_180_degrees() {
        let mut sim = Simulation::new("test", 0, &Code::None);

        // Initial state.
        let ship0 = ship::create(&mut sim, 0.0, 0.0, 0.0, 0.0, 0.0, ship::fighter(0));
        let ship1 = ship::create(&mut sim, 1000.0, 0.0, 0.0, 0.0, 0.0, ship::target(1));
        sim.step();
        assert_eq!(sim.ship(ship0).radar().unwrap().result.is_some(), true);

        // Set width to 180 degrees.
        sim.ship_mut(ship0).radar_mut().unwrap().heading = 0.0;
        sim.ship_mut(ship0).radar_mut().unwrap().width = TAU / 2.0;
        sim.step();
        assert_eq!(sim.ship(ship0).radar().unwrap().result.is_some(), true);

        // Move target north.
        sim.ship_mut(ship1)
            .body()
            .set_translation(vector![EPSILON, 1000.0], true);
        sim.step();
        assert_eq!(sim.ship(ship0).radar().unwrap().result.is_some(), true);

        // Move just out of range to the north west.
        sim.ship_mut(ship1)
            .body()
            .set_translation(vector![-EPSILON, 1000.0], true);
        sim.step();
        assert_eq!(sim.ship(ship0).radar().unwrap().result.is_some(), false);

        // Move target south.
        sim.ship_mut(ship1)
            .body()
            .set_translation(vector![EPSILON, -1000.0], true);
        sim.step();
        assert_eq!(sim.ship(ship0).radar().unwrap().result.is_some(), true);

        // Move just out of range to the south west.
        sim.ship_mut(ship1)
            .body()
            .set_translation(vector![-EPSILON, -1000.0], true);
        sim.step();
        assert_eq!(sim.ship(ship0).radar().unwrap().result.is_some(), false);

        // Move target west.
        sim.ship_mut(ship1)
            .body()
            .set_translation(vector![-1000.0, 0.0], true);
        sim.step();
        assert_eq!(sim.ship(ship0).radar().unwrap().result.is_some(), false);
    }

    #[test]
    fn test_270_degrees() {
        let mut sim = Simulation::new("test", 0, &Code::None);

        // Initial state.
        let ship0 = ship::create(&mut sim, 0.0, 0.0, 0.0, 0.0, 0.0, ship::fighter(0));
        let ship1 = ship::create(&mut sim, 1000.0, 0.0, 0.0, 0.0, 0.0, ship::target(1));
        sim.step();
        assert_eq!(sim.ship(ship0).radar().unwrap().result.is_some(), true);

        // Set width to 270 degrees.
        sim.ship_mut(ship0).radar_mut().unwrap().heading = 0.0;
        sim.ship_mut(ship0).radar_mut().unwrap().width = TAU * 3.0 / 4.0;
        sim.step();
        assert_eq!(sim.ship(ship0).radar().unwrap().result.is_some(), true);

        // Move target up.
        sim.ship_mut(ship1)
            .body()
            .set_translation(vector![0.0, 1000.0], true);
        sim.step();
        assert_eq!(sim.ship(ship0).radar().unwrap().result.is_some(), true);

        // Move target down.
        sim.ship_mut(ship1)
            .body()
            .set_translation(vector![0.0, -1000.0], true);
        sim.step();
        assert_eq!(sim.ship(ship0).radar().unwrap().result.is_some(), true);

        // Move target left.
        sim.ship_mut(ship1)
            .body()
            .set_translation(vector![-1000.0, 100.0], true);
        sim.step();
        assert_eq!(sim.ship(ship0).radar().unwrap().result.is_some(), false);
    }

    #[test]
    fn test_360_degrees() {
        let mut sim = Simulation::new("test", 0, &Code::None);

        // Initial state.
        let ship0 = ship::create(&mut sim, 0.0, 0.0, 0.0, 0.0, 0.0, ship::fighter(0));
        let ship1 = ship::create(&mut sim, 1000.0, 0.0, 0.0, 0.0, 0.0, ship::target(1));
        sim.step();
        assert_eq!(sim.ship(ship0).radar().unwrap().result.is_some(), true);

        // Set width to 360 degrees.
        sim.ship_mut(ship0).radar_mut().unwrap().heading = 0.0;
        sim.ship_mut(ship0).radar_mut().unwrap().width = TAU;
        sim.step();
        assert_eq!(sim.ship(ship0).radar().unwrap().result.is_some(), true);

        // Move target up.
        sim.ship_mut(ship1)
            .body()
            .set_translation(vector![0.0, 1000.0], true);
        sim.step();
        assert_eq!(sim.ship(ship0).radar().unwrap().result.is_some(), true);

        // Move target down.
        sim.ship_mut(ship1)
            .body()
            .set_translation(vector![0.0, -1000.0], true);
        sim.step();
        assert_eq!(sim.ship(ship0).radar().unwrap().result.is_some(), true);

        // Move target left.
        sim.ship_mut(ship1)
            .body()
            .set_translation(vector![-1000.0, 100.0], true);
        sim.step();
        assert_eq!(sim.ship(ship0).radar().unwrap().result.is_some(), true);
    }

    #[test]
    fn test_random() {
        let mut rng = crate::rng::new_rng(1);
        for _ in 0..1000 {
            let mut sim = Simulation::new("test", 0, &Code::None);
            let mut rand_vector =
                || vector![rng.gen_range(-100.0..100.0), rng.gen_range(-100.0..100.0)];
            let p0 = rand_vector();
            let p1 = rand_vector();
            let h = rng.gen_range(0.0..TAU);
            let w = rng.gen_range(0.0..TAU);

            let ship0 = ship::create(&mut sim, p0.x, p0.y, 0.0, 0.0, h, ship::fighter(0));
            let _ship1 = ship::create(&mut sim, p1.x, p1.y, 0.0, 0.0, 0.0, ship::target(1));
            sim.ship_mut(ship0).radar_mut().unwrap().width = w;
            sim.step();

            let dp = p1 - p0;
            let center_vec = UnitComplex::new(h).transform_vector(&vector![1.0, 0.0]);
            let expected = dp.angle(&center_vec).abs() < w * 0.5;
            let got = sim.ship(ship0).radar().unwrap().result.is_some();
            assert_eq!(
                got, expected,
                "p0={:?} p1={:?} h={} w={} expected={} got={}",
                p0, p1, h, w, expected, got
            );
        }
    }
}
