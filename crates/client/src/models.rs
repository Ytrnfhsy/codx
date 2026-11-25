//! Procedural toon models: weapons, helicopter, jets, bomb, grenade (soldiers: `soldier.rs`).

use crate::mesh::{trs, ToonMesh};
use macroquad::prelude::*;
use toon_shared::weapons::WeaponId;
use toon_shared::Team;

const METAL: [u8; 3] = [58, 62, 72];
const WOOD: [u8; 3] = [170, 104, 52];
const TAN: [u8; 3] = [196, 170, 120];
const OLIVE: [u8; 3] = [104, 120, 70];

/// Team colours: a bright one (bubbles, markers) and a deeper one (helicopter paint).
pub struct TeamLook {
    pub helmet: [u8; 3],
    pub vest: [u8; 3],
}

pub fn team_look(t: Team) -> TeamLook {
    match t {
        Team::Allies => TeamLook {
            helmet: [72, 142, 250],
            vest: [52, 96, 170],
        },
        Team::Axis => TeamLook {
            helmet: [240, 92, 68],
            vest: [176, 70, 56],
        },
        Team::Spectator => TeamLook {
            helmet: [200, 200, 200],
            vest: [150, 150, 150],
        },
    }
}

pub fn team_color(t: Team) -> Color {
    match t {
        Team::Allies => Color::from_rgba(90, 168, 255, 255),
        Team::Axis => Color::from_rgba(255, 107, 94, 255),
        Team::Spectator => Color::from_rgba(200, 200, 200, 255),
    }
}

/// One box of a gun model.
#[derive(Clone, Copy)]
pub struct GunPart {
    pub c: Vec3,
    pub h: Vec3,
    pub col: [u8; 3],
    /// Detachable magazine / ammo box / drum (moves during reloads).
    pub mag: bool,
    /// Shotgun pump (slides back when racked).
    pub pump: bool,
}

const BLACK: [u8; 3] = [42, 44, 50];
const SILVER: [u8; 3] = [196, 200, 208];
const DARK_WOOD: [u8; 3] = [120, 72, 40];
const GREEN_BOX: [u8; 3] = [92, 120, 60];
const BAKELITE: [u8; 3] = [170, 90, 50];

/// Builds the chunky toy-like guns, one recognisable silhouette per weapon.
/// Gun space: barrel along -Z, origin at the pistol grip, Y up.
struct Gun(Vec<GunPart>);

impl Gun {
    fn new() -> Gun {
        Gun(Vec::new())
    }
    fn b(&mut self, c: [f32; 3], h: [f32; 3], col: [u8; 3]) -> &mut Self {
        self.0.push(GunPart {
            c: Vec3::from_array(c),
            h: Vec3::from_array(h),
            col,
            mag: false,
            pump: false,
        });
        self
    }
    fn mag(&mut self, c: [f32; 3], h: [f32; 3], col: [u8; 3]) -> &mut Self {
        self.b(c, h, col);
        self.0.last_mut().unwrap().mag = true;
        self
    }
    fn pump(&mut self, c: [f32; 3], h: [f32; 3], col: [u8; 3]) -> &mut Self {
        self.b(c, h, col);
        self.0.last_mut().unwrap().pump = true;
        self
    }
    /// Pistol grip with a trigger guard.
    fn grip(&mut self, col: [u8; 3]) -> &mut Self {
        self.b([0.0, -0.035, 0.02], [0.026, 0.06, 0.03], col).b(
            [0.0, 0.0, -0.05],
            [0.008, 0.016, 0.03],
            BLACK,
        )
    }
    /// Folding bipod under the barrel.
    fn bipod(&mut self, z: f32) -> &mut Self {
        self.b([0.035, -0.02, z], [0.008, 0.075, 0.008], BLACK)
            .b([-0.035, -0.02, z], [0.008, 0.075, 0.008], BLACK)
            .b([0.0, 0.05, z], [0.035, 0.008, 0.012], BLACK)
    }
    /// Telescopic sight above the receiver.
    fn scope(&mut self, z: f32, col: [u8; 3]) -> &mut Self {
        self.b([0.0, 0.155, z], [0.022, 0.022, 0.16], col)
            .b([0.0, 0.155, z - 0.17], [0.032, 0.032, 0.03], col)
            .b([0.0, 0.155, z + 0.15], [0.028, 0.028, 0.025], col)
            .b([0.0, 0.115, z - 0.07], [0.012, 0.02, 0.012], BLACK)
            .b([0.0, 0.115, z + 0.07], [0.012, 0.02, 0.012], BLACK)
    }
}

fn gun_def(w: WeaponId) -> Gun {
    let mut g = Gun::new();
    match w {
        // ---------------------------------------------------------------- assault rifles
        WeaponId::Uar15 => {
            g.b([0.0, 0.07, -0.1], [0.04, 0.05, 0.17], BLACK) // receiver
                .b([0.0, 0.125, -0.2], [0.016, 0.008, 0.25], BLACK) // top rail
                .b([0.0, 0.15, -0.08], [0.02, 0.022, 0.035], BLACK) // red dot
                .b([0.0, 0.152, -0.115], [0.016, 0.016, 0.002], [120, 200, 240])
                .b([0.0, 0.07, -0.4], [0.044, 0.042, 0.15], METAL) // long handguard
                .b([0.0, 0.07, -0.63], [0.015, 0.015, 0.09], BLACK) // barrel
                .b([0.0, 0.07, -0.74], [0.022, 0.022, 0.025], BLACK) // brake
                .b([0.0, 0.065, 0.12], [0.018, 0.018, 0.08], BLACK) // buffer tube
                .b([0.0, 0.045, 0.22], [0.035, 0.065, 0.05], METAL) // stock
                .grip(BLACK)
                .mag([0.0, -0.06, -0.15], [0.026, 0.07, 0.035], METAL)
                .mag([0.0, -0.14, -0.17], [0.026, 0.025, 0.035], METAL);
        }
        WeaponId::Fort221 => {
            // a bullpup: the magazine behind the grip, a big trigger guard in front
            let c = [120, 122, 88];
            g.b([0.0, 0.05, -0.04], [0.045, 0.07, 0.26], c) // body
                .b([0.0, 0.13, -0.1], [0.016, 0.008, 0.2], BLACK) // rail
                .b([0.0, 0.16, -0.12], [0.022, 0.024, 0.04], BLACK) // sight
                .b([0.0, 0.07, -0.36], [0.016, 0.016, 0.07], BLACK) // barrel
                .b([0.0, 0.07, -0.44], [0.02, 0.02, 0.02], BLACK)
                .b([0.0, -0.04, -0.14], [0.03, 0.045, 0.045], c) // trigger guard
                .b([0.0, 0.03, 0.22], [0.045, 0.08, 0.02], BLACK) // butt pad
                .grip(c)
                .mag([0.0, -0.08, 0.1], [0.026, 0.07, 0.035], METAL);
        }
        WeaponId::Ak12 => {
            g.b([0.0, 0.07, -0.1], [0.04, 0.045, 0.17], METAL)
                .b([0.0, 0.125, -0.2], [0.016, 0.008, 0.26], BLACK) // rail
                .b([0.0, 0.118, -0.36], [0.018, 0.016, 0.1], METAL) // gas tube
                .b([0.0, 0.065, -0.36], [0.044, 0.038, 0.1], BLACK) // handguard
                .b([0.0, 0.07, -0.56], [0.015, 0.015, 0.11], METAL)
                .b([0.0, 0.07, -0.69], [0.026, 0.026, 0.03], BLACK) // big brake
                .b([0.0, 0.07, 0.12], [0.014, 0.014, 0.07], BLACK) // folding stock
                .b([0.0, 0.04, 0.22], [0.032, 0.065, 0.05], BLACK)
                .grip(BLACK)
                .mag([0.0, -0.05, -0.16], [0.026, 0.06, 0.035], [60, 60, 66])
                .mag([0.0, -0.12, -0.2], [0.026, 0.035, 0.035], [60, 60, 66]);
        }
        WeaponId::ScarH => {
            g.b([0.0, 0.075, -0.1], [0.042, 0.055, 0.2], TAN) // receiver
                .b([0.0, 0.135, -0.2], [0.016, 0.008, 0.26], BLACK)
                .b([0.0, 0.07, -0.42], [0.046, 0.046, 0.12], TAN) // handguard
                .b([0.0, 0.075, -0.62], [0.017, 0.017, 0.09], BLACK)
                .b([0.0, 0.075, -0.73], [0.024, 0.024, 0.025], BLACK)
                .b([0.0, 0.055, 0.2], [0.035, 0.07, 0.13], TAN) // folding stock
                .b([0.0, 0.16, 0.02], [0.012, 0.02, 0.012], BLACK) // rear sight
                .grip(BLACK)
                .mag([0.0, -0.055, -0.14], [0.028, 0.055, 0.035], TAN);
        }
        // ---------------------------------------------------------------- SMGs
        WeaponId::Vector => {
            // the odd one: a slanted lower with the magazine in front
            g.b([0.0, 0.08, -0.1], [0.035, 0.04, 0.17], BLACK)
                .b([0.0, 0.0, -0.17], [0.032, 0.06, 0.07], BLACK) // slanted lower
                .b([0.0, 0.125, -0.12], [0.014, 0.008, 0.14], BLACK)
                .b([0.0, 0.08, -0.31], [0.016, 0.016, 0.05], BLACK) // barrel
                .b([0.0, 0.08, -0.37], [0.022, 0.022, 0.02], BLACK)
                .b([0.0, 0.07, 0.14], [0.012, 0.032, 0.09], METAL) // skeleton stock
                .b([0.0, 0.05, 0.23], [0.03, 0.055, 0.012], BLACK)
                .grip(BLACK)
                .mag([0.0, -0.11, -0.22], [0.022, 0.08, 0.03], METAL);
        }
        WeaponId::Mp7 => {
            g.b([0.0, 0.07, -0.06], [0.035, 0.045, 0.13], BLACK)
                .b([0.0, 0.122, -0.08], [0.014, 0.008, 0.1], BLACK)
                .b([0.0, 0.07, -0.22], [0.013, 0.013, 0.04], BLACK)
                .b([0.0, 0.0, -0.17], [0.014, 0.05, 0.015], BLACK) // folding foregrip
                .b([0.0, 0.065, 0.11], [0.01, 0.01, 0.07], METAL) // retracted stock
                .b([0.0, 0.05, 0.18], [0.028, 0.04, 0.01], BLACK)
                .grip(BLACK)
                .mag([0.0, -0.12, 0.02], [0.022, 0.03, 0.026], METAL); // in the grip
        }
        WeaponId::Bizon => {
            g.b([0.0, 0.07, -0.08], [0.04, 0.045, 0.15], METAL)
                .b([0.0, 0.07, -0.42], [0.015, 0.015, 0.09], METAL)
                .b([0.0, 0.12, -0.43], [0.01, 0.03, 0.01], METAL)
                .b([0.032, 0.06, 0.13], [0.008, 0.008, 0.11], METAL) // folding stock
                .b([0.0, 0.03, 0.23], [0.032, 0.05, 0.012], METAL)
                .grip(BAKELITE)
                // the helical magazine under the barrel
                .mag([0.0, 0.005, -0.3], [0.034, 0.034, 0.15], BLACK)
                .mag([0.0, 0.005, -0.3], [0.036, 0.008, 0.15], [70, 72, 80]);
        }
        // ---------------------------------------------------------------- marksman rifles
        WeaponId::Svd => {
            g.b([0.0, 0.075, -0.08], [0.036, 0.04, 0.16], METAL)
                .b([0.0, 0.07, -0.36], [0.04, 0.036, 0.12], WOOD) // handguard
                .b([0.0, 0.075, -0.66], [0.014, 0.014, 0.18], METAL) // long barrel
                .b([0.0, 0.075, -0.86], [0.02, 0.02, 0.025], METAL)
                .b([0.0, 0.075, 0.16], [0.03, 0.018, 0.11], WOOD) // skeleton stock
                .b([0.0, -0.005, 0.18], [0.03, 0.018, 0.08], WOOD)
                .b([0.0, 0.03, 0.28], [0.032, 0.075, 0.025], WOOD)
                .grip(WOOD)
                .scope(-0.06, BLACK)
                .mag([0.0, -0.04, -0.12], [0.024, 0.05, 0.035], METAL);
        }
        WeaponId::Mk14 => {
            g.b([0.0, 0.075, -0.08], [0.04, 0.05, 0.18], METAL)
                .b([0.0, 0.065, -0.34], [0.048, 0.05, 0.12], TAN) // chassis
                .b([0.0, 0.13, -0.2], [0.016, 0.008, 0.25], BLACK)
                .b([0.0, 0.16, -0.06], [0.02, 0.024, 0.06], BLACK) // short scope
                .b([0.0, 0.075, -0.6], [0.015, 0.015, 0.13], BLACK)
                .b([0.0, 0.075, -0.75], [0.023, 0.023, 0.025], BLACK)
                .b([0.0, 0.06, 0.2], [0.035, 0.065, 0.12], TAN)
                .grip(BLACK)
                .mag([0.0, -0.05, -0.12], [0.026, 0.05, 0.035], METAL);
        }
        // ---------------------------------------------------------------- machine guns
        WeaponId::Negev => {
            g.b([0.0, 0.08, -0.06], [0.05, 0.06, 0.2], METAL)
                .b([0.0, 0.15, -0.04], [0.045, 0.016, 0.12], BLACK) // feed cover
                .b([0.0, 0.07, -0.36], [0.046, 0.042, 0.1], BLACK)
                .b([0.0, 0.075, -0.62], [0.017, 0.017, 0.16], BLACK)
                .b([0.0, 0.075, -0.8], [0.024, 0.024, 0.02], BLACK)
                .bipod(-0.56)
                .b([0.0, 0.06, 0.21], [0.03, 0.065, 0.13], BLACK)
                .grip(BLACK)
                .mag([0.0, -0.05, -0.1], [0.048, 0.07, 0.07], OLIVE);
        }
        WeaponId::Pkm => {
            g.b([0.0, 0.08, -0.05], [0.045, 0.06, 0.18], METAL)
                .b([0.0, 0.14, -0.02], [0.04, 0.012, 0.12], METAL)
                .b([0.0, 0.08, -0.6], [0.016, 0.016, 0.24], METAL) // long barrel
                .b([0.0, 0.05, -0.45], [0.012, 0.012, 0.15], METAL) // gas tube
                .b([0.0, 0.12, -0.6], [0.014, 0.012, 0.035], DARK_WOOD) // carry handle
                .b([0.0, 0.08, -0.86], [0.022, 0.022, 0.025], METAL)
                .bipod(-0.62)
                .b([0.0, 0.07, 0.2], [0.03, 0.02, 0.13], DARK_WOOD) // skeleton stock
                .b([0.0, -0.01, 0.2], [0.03, 0.02, 0.1], DARK_WOOD)
                .b([0.0, 0.03, 0.32], [0.032, 0.07, 0.02], DARK_WOOD)
                .grip(DARK_WOOD)
                .mag([0.0, -0.06, -0.08], [0.04, 0.06, 0.07], GREEN_BOX);
        }
        // ---------------------------------------------------------------- shotguns
        WeaponId::Aa12 => {
            g.b([0.0, 0.06, -0.08], [0.045, 0.06, 0.2], BLACK) // boxy body
                .b([0.0, 0.14, -0.1], [0.014, 0.022, 0.1], BLACK) // top handle
                .b([0.0, 0.06, -0.38], [0.032, 0.032, 0.1], [70, 72, 80]) // shroud
                .b([0.0, 0.06, -0.5], [0.022, 0.022, 0.02], BLACK)
                .b([0.0, 0.05, 0.18], [0.04, 0.06, 0.09], BLACK)
                .grip(BLACK)
                .mag([0.0, -0.08, -0.13], [0.05, 0.065, 0.065], BLACK); // drum
        }
        WeaponId::Ksg => {
            // a bullpup pump with two tubes under the barrel
            g.b([0.0, 0.05, -0.04], [0.045, 0.065, 0.24], BLACK)
                .b([0.0, 0.13, -0.12], [0.014, 0.008, 0.18], BLACK)
                .b([0.0, 0.08, -0.38], [0.016, 0.016, 0.15], METAL) // barrel
                .b([0.018, 0.035, -0.35], [0.014, 0.014, 0.13], METAL) // tube 1
                .b([-0.018, 0.035, -0.35], [0.014, 0.014, 0.13], METAL) // tube 2
                .pump([0.0, 0.0, -0.32], [0.032, 0.03, 0.07], BLACK)
                .b([0.0, -0.03, -0.32], [0.014, 0.04, 0.014], BLACK) // pump grip
                .b([0.0, 0.03, 0.21], [0.045, 0.08, 0.02], BLACK)
                .grip(BLACK)
                .mag([0.0, -0.06, 0.1], [0.028, 0.03, 0.04], METAL);
        }
        // ---------------------------------------------------------------- sniper rifles
        WeaponId::Awm => {
            let green = [86, 104, 72];
            g.b([0.0, 0.035, -0.06], [0.038, 0.048, 0.26], green) // stock & forend
                .b([0.0, 0.0, 0.25], [0.038, 0.08, 0.07], green) // thumbhole butt
                .b([0.0, 0.085, -0.05], [0.03, 0.025, 0.12], METAL) // action
                .b([0.05, 0.085, 0.04], [0.03, 0.01, 0.01], METAL) // bolt handle
                .b([0.075, 0.085, 0.04], [0.012, 0.014, 0.014], METAL)
                .b([0.0, 0.085, -0.52], [0.015, 0.015, 0.26], BLACK)
                .b([0.0, 0.085, -0.8], [0.026, 0.022, 0.03], BLACK) // brake
                .scope(-0.06, BLACK)
                .mag([0.0, -0.01, -0.08], [0.02, 0.03, 0.035], METAL);
        }
        WeaponId::Barrett => {
            g.b([0.0, 0.07, -0.05], [0.05, 0.06, 0.3], BLACK) // big receiver
                .b([0.0, 0.075, -0.62], [0.022, 0.022, 0.26], BLACK) // thick barrel
                .b([0.0, 0.075, -0.92], [0.045, 0.035, 0.04], BLACK) // huge brake
                .b([0.0, 0.03, 0.3], [0.045, 0.08, 0.04], BLACK) // butt
                .scope(-0.05, BLACK)
                .bipod(-0.48)
                .grip(BLACK)
                .mag([0.0, -0.06, -0.12], [0.032, 0.06, 0.045], METAL);
        }
        WeaponId::Alligator => {
            let c = [96, 104, 82];
            g.b([0.0, 0.07, 0.0], [0.05, 0.065, 0.28], c) // long body
                .b([0.0, 0.075, -0.66], [0.026, 0.026, 0.34], METAL) // huge barrel
                .b([0.0, 0.075, -1.02], [0.052, 0.042, 0.05], METAL) // massive brake
                .b([0.0, 0.02, 0.33], [0.048, 0.09, 0.04], BLACK) // butt
                .b([0.0, -0.07, 0.26], [0.012, 0.07, 0.012], BLACK) // rear monopod
                .scope(-0.0, BLACK)
                .bipod(-0.55)
                .grip(BLACK)
                .mag([0.0, -0.07, 0.08], [0.036, 0.07, 0.05], METAL);
        }
        // ---------------------------------------------------------------- pistols
        WeaponId::Fort17 => {
            let c = [86, 92, 70];
            g.b([0.0, 0.07, -0.08], [0.025, 0.03, 0.12], BLACK) // slide
                .b([0.0, 0.035, -0.07], [0.023, 0.016, 0.1], c)
                .b([0.0, 0.11, -0.19], [0.006, 0.012, 0.006], BLACK)
                .b([0.0, -0.03, 0.02], [0.025, 0.06, 0.03], c)
                .b([0.0, 0.0, -0.06], [0.008, 0.016, 0.028], BLACK)
                .mag([0.0, -0.095, 0.02], [0.022, 0.012, 0.028], BLACK);
        }
        WeaponId::Glock18 => {
            g.b([0.0, 0.07, -0.08], [0.026, 0.032, 0.125], BLACK)
                .b([0.0, 0.035, -0.07], [0.024, 0.017, 0.1], [60, 60, 66])
                .b([0.0, -0.03, 0.02], [0.026, 0.062, 0.032], [60, 60, 66])
                .b([0.0, 0.0, -0.06], [0.008, 0.016, 0.028], BLACK)
                .b([0.028, 0.08, 0.0], [0.004, 0.01, 0.01], [220, 60, 50]) // the auto switch
                .mag([0.0, -0.13, 0.02], [0.023, 0.04, 0.029], BLACK); // long magazine
        }
        WeaponId::Revolver => {
            g.b([0.0, 0.075, -0.17], [0.012, 0.014, 0.09], SILVER) // barrel
                .b([0.0, 0.055, -0.1], [0.01, 0.01, 0.06], SILVER) // ejector rod
                .b([0.0, 0.06, -0.03], [0.02, 0.04, 0.06], SILVER) // frame
                .b([0.0, 0.1, 0.04], [0.006, 0.014, 0.012], BLACK) // hammer
                .b([0.0, -0.035, 0.03], [0.022, 0.06, 0.03], DARK_WOOD)
                .b([0.0, 0.0, -0.06], [0.007, 0.016, 0.025], SILVER)
                .mag([0.0, 0.06, -0.04], [0.03, 0.03, 0.035], SILVER); // the cylinder
        }
        // ---------------------------------------------------------------- launchers
        WeaponId::Rpg7 => {
            g.b([0.0, 0.09, -0.12], [0.03, 0.03, 0.42], OLIVE) // the tube
                .b([0.0, 0.09, -0.08], [0.037, 0.037, 0.1], WOOD) // heat guard
                .b([0.0, 0.09, 0.33], [0.045, 0.045, 0.03], METAL) // rear flare
                .b([0.0, 0.14, -0.14], [0.01, 0.025, 0.02], BLACK) // sight
                .b([0.0, 0.03, -0.2], [0.014, 0.05, 0.02], WOOD) // front grip
                .grip(WOOD)
                // the rocket sticking out of the front
                .mag([0.0, 0.09, -0.6], [0.022, 0.022, 0.06], [90, 100, 66])
                .mag([0.0, 0.09, -0.7], [0.055, 0.055, 0.05], [90, 100, 66])
                .mag([0.0, 0.09, -0.79], [0.03, 0.03, 0.04], [70, 78, 52]);
        }
        WeaponId::Crossbow => {
            g.b([0.0, 0.05, 0.0], [0.03, 0.045, 0.22], WOOD) // stock
                .b([0.0, 0.08, -0.27], [0.02, 0.015, 0.15], METAL) // rail
                .b([0.0, 0.08, -0.4], [0.3, 0.02, 0.025], BLACK) // limbs
                .b([0.0, 0.085, -0.34], [0.28, 0.004, 0.004], SILVER) // string
                .b([0.0, 0.13, -0.12], [0.018, 0.02, 0.05], BLACK) // scope
                .grip(WOOD)
                .mag([0.0, 0.1, -0.3], [0.006, 0.006, 0.17], [150, 110, 60]) // the bolt
                .mag([0.0, 0.1, -0.13], [0.004, 0.02, 0.02], [220, 60, 50]);
        }
        WeaponId::Bow => {
            // held upright by the riser; the arrow along -Z
            g.b([0.0, 0.05, 0.0], [0.02, 0.07, 0.03], DARK_WOOD) // riser
                .b([0.0, 0.25, 0.02], [0.014, 0.14, 0.02], WOOD) // upper limb
                .b([0.0, 0.41, 0.06], [0.01, 0.04, 0.016], WOOD)
                .b([0.0, -0.15, 0.02], [0.014, 0.14, 0.02], WOOD) // lower limb
                .b([0.0, -0.31, 0.06], [0.01, 0.04, 0.016], WOOD)
                .b([0.0, 0.05, 0.08], [0.003, 0.38, 0.003], SILVER) // the string
                .mag([0.0, 0.05, -0.12], [0.005, 0.005, 0.3], [150, 110, 60]) // the arrow
                .mag([0.0, 0.05, -0.43], [0.012, 0.012, 0.02], METAL)
                .mag([0.0, 0.05, 0.15], [0.003, 0.02, 0.03], [220, 60, 50]);
        }
    }
    g
}

/// Gun parts in gun space: barrel along -Z, origin at the grip.
pub fn gun_parts(w: WeaponId) -> Vec<GunPart> {
    gun_def(w).0
}

/// A sight found in battle royale (1 red dot, 2..4 scopes), standing on `y`.
pub fn sight_parts(level: u8, y: f32) -> Vec<GunPart> {
    let mut g = Gun::new();
    match level {
        0 => {}
        1 => {
            // a mount and an open frame: you look through it (the dot is drawn on the HUD)
            let (cy, half, bar) = (y + 0.042, 0.022, 0.0035);
            g.b([0.0, y + 0.01, -0.06], [0.016, 0.01, 0.035], BLACK)
                .b([-half, cy, -0.07], [bar, half, 0.006], BLACK)
                .b([half, cy, -0.07], [bar, half, 0.006], BLACK)
                .b([0.0, cy + half, -0.07], [half + bar, bar, 0.006], BLACK)
                .b([0.0, cy - half, -0.07], [half + bar, bar, 0.006], BLACK);
        }
        l => {
            // longer and thicker the stronger it is
            let r = 0.017 + 0.004 * l as f32;
            let len = 0.06 + 0.025 * l as f32;
            let cy = y + 0.022 + r;
            g.b([0.0, y + 0.012, -0.11], [0.01, 0.014, 0.01], BLACK)
                .b([0.0, y + 0.012, 0.0], [0.01, 0.014, 0.01], BLACK)
                .b([0.0, cy, -0.055], [r, r, len], BLACK)
                .b(
                    [0.0, cy, -0.055 - len],
                    [r + 0.008, r + 0.008, 0.018],
                    BLACK,
                )
                .b(
                    [0.0, cy, -0.055 + len],
                    [r + 0.004, r + 0.004, 0.012],
                    BLACK,
                )
                .b(
                    [0.0, cy, -0.055 - len - 0.019],
                    [r, r, 0.002],
                    [90, 140, 200],
                );
        }
    }
    g.0
}

/// A gun with the sight it carries (snipers keep their own scope).
pub fn gun_parts_with(w: WeaponId, sight: u8) -> Vec<GunPart> {
    let mut parts = gun_parts(w);
    if sight == 0 || w.def().scoped {
        return parts;
    }
    // on top of the receiver (iron sights stick out further forward)
    let top = parts
        .iter()
        .filter(|p| p.c.z > -0.25 && p.c.z < 0.1 && !p.mag)
        .map(|p| p.c.y + p.h.y)
        .fold(0.06, f32::max)
        .min(0.13);
    parts.extend(sight_parts(sight, top));
    parts
}

/// A gun with its sight and other attachments: a suppressor or compensator on the muzzle,
/// a longer magazine, a grip under the barrel.
pub fn gun_parts_full(w: WeaponId, sight: u8, mods: [u8; 3]) -> Vec<GunPart> {
    use toon_shared::inventory::{MOD_COMPENSATOR, MOD_EXTMAG, MOD_GRIP, MOD_SUPPRESSOR};
    let mut parts = gun_parts_with(w, sight);
    let mz = muzzle(w);
    match mods[0] {
        MOD_SUPPRESSOR => parts.push(GunPart {
            c: mz + vec3(0.0, 0.0, -0.1),
            h: vec3(0.024, 0.024, 0.1),
            col: [36, 38, 42],
            mag: false,
            pump: false,
        }),
        MOD_COMPENSATOR => parts.push(GunPart {
            c: mz + vec3(0.0, 0.0, -0.03),
            h: vec3(0.026, 0.026, 0.035),
            col: [70, 72, 78],
            mag: false,
            pump: false,
        }),
        _ => {}
    }
    if mods[1] == MOD_EXTMAG {
        for p in parts.iter_mut().filter(|p| p.mag) {
            p.c.y -= p.h.y * 0.5;
            p.h.y *= 1.5;
        }
    }
    if mods[2] == MOD_GRIP {
        let g = fore_grip(w);
        parts.push(GunPart {
            c: g + vec3(0.0, -0.065, 0.0),
            h: vec3(0.016, 0.05, 0.018),
            col: [42, 44, 50],
            mag: false,
            pump: false,
        });
    }
    parts
}

/// Centre of the magazine (where the off hand goes when reloading).
pub fn gun_mag(w: WeaponId) -> Option<(Vec3, Vec3, [u8; 3])> {
    gun_parts(w)
        .iter()
        .find(|p| p.mag)
        .map(|p| (p.c, p.h, p.col))
}

/// Where the off hand holds the gun.
pub fn fore_grip(w: WeaponId) -> Vec3 {
    use toon_shared::weapons::WeaponClass as C;
    match w {
        WeaponId::Mp7 => vec3(0.0, -0.02, -0.17),
        WeaponId::Vector => vec3(0.0, -0.02, -0.2),
        WeaponId::Bizon => vec3(0.0, 0.0, -0.3),
        WeaponId::Fort221 | WeaponId::Ksg => vec3(0.0, -0.03, -0.3),
        WeaponId::Aa12 => vec3(0.0, 0.02, -0.36),
        WeaponId::Rpg7 => vec3(0.0, 0.0, -0.2),
        WeaponId::Crossbow => vec3(0.0, 0.03, -0.24),
        WeaponId::Bow => vec3(0.0, 0.05, 0.12),
        WeaponId::Barrett | WeaponId::Alligator => vec3(0.0, 0.03, -0.38),
        _ => match w.def().class {
            C::Pistol => vec3(-0.02, -0.03, 0.03),
            C::Sniper | C::Marksman => vec3(0.0, 0.0, -0.26),
            _ => vec3(0.0, 0.03, -0.36),
        },
    }
}

/// Muzzle position in gun space: the front of the part furthest forward.
pub fn muzzle(w: WeaponId) -> Vec3 {
    gun_parts(w)
        .iter()
        .filter(|p| !p.mag || w.def().class == toon_shared::weapons::WeaponClass::Launcher)
        .min_by(|a, b| (a.c.z - a.h.z).total_cmp(&(b.c.z - b.h.z)))
        .map(|p| vec3(0.0, p.c.y, p.c.z - p.h.z))
        .unwrap_or(vec3(0.0, 0.07, -0.7))
}

pub fn add_gun(m: &mut ToonMesh, base: &Mat4, w: WeaponId, outline: f32) {
    add_gun_ex(m, base, w, outline, Vec3::ZERO);
}

/// Gun with its magazine moved by `mag_off` (gun space) — used by reload animations.
pub fn add_gun_ex(m: &mut ToonMesh, base: &Mat4, w: WeaponId, outline: f32, mag_off: Vec3) {
    for p in gun_parts(w) {
        let c = if p.mag { p.c + mag_off } else { p.c };
        m.boxed(&(*base * Mat4::from_translation(c)), p.h, p.col, outline);
    }
}

/// A weapon lying on the ground. `age` drives the short fall when it was just dropped;
/// `highlight` makes it bob a little when it can be picked up.
pub fn add_dropped_gun(
    m: &mut ToonMesh,
    pos: Vec3,
    yaw: f32,
    w: WeaponId,
    age: f32,
    highlight: bool,
) {
    let fall = (1.0 - age / 0.4).max(0.0);
    let lift = 1.0 * fall * fall
        + if highlight {
            0.06 + (age * 4.0).sin().abs() * 0.05
        } else {
            0.0
        };
    let base = trs(
        pos + vec3(0.0, 0.07 + lift, 0.0),
        yaw + fall * 3.0,
        0.0,
        std::f32::consts::FRAC_PI_2 * (1.0 - fall * 0.6),
    ) * Mat4::from_scale(Vec3::splat(1.25));
    add_gun(m, &base, w, if highlight { 0.03 } else { 0.016 });
}

/// Team colour of a character's silhouette outline.
pub fn team_ink(t: Team) -> [u8; 3] {
    match t {
        Team::Allies => [70, 150, 255],
        Team::Axis => [255, 78, 64],
        Team::Spectator => [24, 22, 30],
    }
}

pub fn add_grenade(m: &mut ToonMesh, pos: Vec3, t: f32) {
    let base = trs(pos, t * 5.0, t * 7.0, 0.0);
    m.sphere(&base, 0.11, [70, 110, 60], 0.012);
    m.boxed(
        &(base * Mat4::from_translation(vec3(0.0, 0.12, 0.0))),
        vec3(0.03, 0.03, 0.03),
        METAL,
        0.01,
    );
}

/// Smoke grenade: a grey can with a white band.
pub fn add_smoke_grenade(m: &mut ToonMesh, pos: Vec3, t: f32) {
    let base = trs(pos, t * 5.0, t * 7.0, 0.0);
    m.boxed(&base, vec3(0.07, 0.12, 0.07), [120, 126, 132], 0.012);
    m.boxed(&base, vec3(0.075, 0.025, 0.075), [235, 235, 230], 0.0);
    m.boxed(
        &(base * Mat4::from_translation(vec3(0.0, 0.14, 0.0))),
        vec3(0.03, 0.03, 0.03),
        METAL,
        0.01,
    );
}

/// A cartoon smoke cloud: a heap of soft grey puffs that swell, drift and thin out.
/// Opaque on purpose: it must really hide whoever is inside or behind it.
pub fn add_smoke_cloud(m: &mut ToonMesh, centre: Vec3, age: f32, id: u16, now: f32) {
    let r = toon_shared::consts::smoke_radius(age);
    if r < 0.05 {
        return;
    }
    let prev = (m.detail, m.ink);
    m.detail = 5;
    // a soft grey line around each puff gives the cartoon-cloud look
    m.ink = Some([150, 156, 166]);
    let mut rng = toon_shared::rng::Rng::new(id as u64 * 7919 + 13);
    for k in 0..24 {
        // puffs spread through the cloud, flattened a little, never below the ground
        let dir = vec3(
            rng.range(-1.0, 1.0),
            rng.range(-0.5, 0.9),
            rng.range(-1.0, 1.0),
        );
        let dir = dir.normalize_or_zero() * rng.range(0.25, 0.7);
        let drift = vec3(
            (now * 0.21 + k as f32).sin(),
            (now * 0.17 + k as f32 * 1.7).sin() * 0.5,
            (now * 0.19 + k as f32 * 2.3).cos(),
        ) * 0.1
            * r;
        let mut c = centre + dir * r + drift;
        let pr = r * rng.range(0.3, 0.45) * (1.0 + 0.06 * (now * 0.9 + k as f32).sin());
        c.y =
            c.y.max(centre.y - toon_shared::consts::SMOKE_LIFT + pr * 0.6);
        let g = rng.range(222.0, 242.0) as u8;
        m.sphere(
            &Mat4::from_translation(c),
            pr,
            [g, g, g.saturating_add(4)],
            0.05,
        );
    }
    (m.detail, m.ink) = prev;
}

pub fn add_bomb(m: &mut ToonMesh, pos: Vec3, t: f32, planted: bool) {
    let base = trs(
        pos + vec3(0.0, 0.22, 0.0),
        t * if planted { 0.0 } else { 1.2 },
        0.0,
        0.0,
    );
    m.boxed(&base, vec3(0.32, 0.2, 0.22), [214, 54, 46], 0.02);
    m.boxed(
        &(base * Mat4::from_translation(vec3(0.0, 0.0, 0.0))),
        vec3(0.33, 0.06, 0.23),
        [250, 220, 70],
        0.0,
    );
    let light = if planted && (t * 4.0).fract() < 0.5 {
        [255, 255, 120]
    } else {
        [120, 30, 30]
    };
    m.boxed(
        &(base * Mat4::from_translation(vec3(0.0, 0.24, 0.0))),
        vec3(0.06, 0.05, 0.06),
        light,
        0.015,
    );
}

pub fn add_heli(m: &mut ToonMesh, pos: Vec3, yaw: f32, team: Team, t: f32) {
    let look = team_look(team);
    let base = trs(pos, yaw, 0.08, 0.0);
    m.boxed(&base, vec3(1.2, 1.0, 2.0), look.vest, 0.06);
    m.sphere(
        &(base
            * Mat4::from_translation(vec3(0.0, 0.1, -1.8))
            * Mat4::from_scale(vec3(1.0, 0.8, 0.9))),
        1.0,
        [160, 220, 250],
        0.06,
    );
    m.boxed(
        &(base * Mat4::from_translation(vec3(0.0, 0.3, 3.2))),
        vec3(0.25, 0.3, 1.8),
        look.vest,
        0.05,
    );
    m.boxed(
        &(base * Mat4::from_translation(vec3(0.0, 0.8, 4.8))),
        vec3(0.1, 0.7, 0.4),
        look.helmet,
        0.05,
    );
    for s in [-1.0f32, 1.0] {
        m.boxed(
            &(base * Mat4::from_translation(vec3(s * 0.9, -1.3, 0.0))),
            vec3(0.08, 0.08, 1.6),
            METAL,
            0.04,
        );
        m.boxed(
            &(base * Mat4::from_translation(vec3(s * 1.5, -0.3, -0.3))),
            vec3(0.3, 0.25, 0.7),
            METAL,
            0.04,
        );
    }
    m.boxed(
        &(base * Mat4::from_translation(vec3(0.0, 1.2, 0.0))),
        vec3(0.2, 0.25, 0.2),
        METAL,
        0.04,
    );
    for k in 0..2 {
        let r = base
            * Mat4::from_translation(vec3(0.0, 1.45, 0.0))
            * Mat4::from_rotation_y(t * 25.0 + k as f32 * std::f32::consts::FRAC_PI_2);
        m.boxed(&r, vec3(0.18, 0.03, 4.2), [40, 40, 46], 0.03);
    }
    let tr = base * Mat4::from_translation(vec3(0.25, 0.9, 4.8)) * Mat4::from_rotation_x(t * 30.0);
    m.boxed(&tr, vec3(0.03, 0.9, 0.12), [40, 40, 46], 0.0);
}

pub fn add_jet(m: &mut ToonMesh, pos: Vec3, dir: Vec3) {
    let yaw = toon_shared::yaw_of(dir);
    let base = trs(pos, yaw, 0.0, 0.0);
    let grey = [150, 160, 176];
    m.boxed(&base, vec3(0.7, 0.6, 4.0), grey, 0.08);
    m.sphere(
        &(base
            * Mat4::from_translation(vec3(0.0, 0.4, -2.2))
            * Mat4::from_scale(vec3(0.6, 0.5, 1.2))),
        1.0,
        [120, 200, 250],
        0.06,
    );
    m.boxed(
        &(base * Mat4::from_translation(vec3(0.0, 0.0, 0.5))),
        vec3(4.5, 0.12, 1.2),
        grey,
        0.06,
    );
    m.boxed(
        &(base * Mat4::from_translation(vec3(0.0, 1.0, 3.4))),
        vec3(0.1, 0.9, 0.6),
        grey,
        0.05,
    );
    m.boxed(
        &(base * Mat4::from_translation(vec3(0.0, 0.1, 3.6))),
        vec3(1.8, 0.1, 0.5),
        grey,
        0.05,
    );
    m.fill.add_sphere_ex(
        &(base * Mat4::from_translation(vec3(0.0, 0.0, 4.3))),
        0.5,
        [255, 170, 60],
        8,
        5,
        255,
        true,
    );
}

/// FPV kamikaze drone: an X frame with four rotors, a camera and a charge strapped
/// underneath. Drawn about twice life size so it can be spotted (and shot).
pub fn add_fpv(m: &mut ToonMesh, pos: Vec3, yaw: f32, pitch: f32, team: Team, t: f32) {
    let prev = m.ink.replace(team_ink(team));
    let base = trs(pos, yaw, pitch, 0.0) * Mat4::from_scale(Vec3::splat(1.6));
    let dark = [52, 54, 60];
    m.boxed(&base, vec3(0.07, 0.035, 0.11), dark, 0.012);
    m.boxed(
        &(base * Mat4::from_translation(vec3(0.0, -0.06, 0.01))),
        vec3(0.05, 0.04, 0.12),
        [104, 120, 70],
        0.01,
    );
    m.boxed(
        &(base * Mat4::from_translation(vec3(0.0, 0.02, -0.12))),
        vec3(0.025, 0.025, 0.02),
        [30, 30, 34],
        0.0,
    );
    for (sx, sz) in [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        let arm = base
            * Mat4::from_translation(vec3(sx * 0.07, 0.0, sz * 0.07))
            * Mat4::from_rotation_y(if sx * sz > 0.0 { -0.785 } else { 0.785 });
        m.boxed(&arm, vec3(0.012, 0.01, 0.1), dark, 0.0);
        let tip = base * Mat4::from_translation(vec3(sx * 0.14, 0.02, sz * 0.14));
        m.boxed(&tip, vec3(0.02, 0.02, 0.02), METAL, 0.0);
        // a spinning rotor reads as a pale disc
        let spin =
            tip * Mat4::from_translation(vec3(0.0, 0.025, 0.0)) * Mat4::from_rotation_y(t * 40.0);
        m.sphere(
            &(spin * Mat4::from_scale(vec3(1.0, 0.08, 1.0))),
            0.075,
            [205, 210, 218],
            0.0,
        );
    }
    m.boxed(
        &(base * Mat4::from_translation(vec3(0.0, 0.04, 0.0))),
        vec3(0.04, 0.01, 0.06),
        team_look(team).helmet,
        0.0,
    );
    m.ink = prev;
}

/// Bomber drone: a big hexacopter with a rack of bombs (`ammo` still hanging).
pub fn add_bomber(m: &mut ToonMesh, pos: Vec3, yaw: f32, team: Team, t: f32, ammo: u8) {
    let prev = m.ink.replace(team_ink(team));
    let base = trs(pos, yaw, 0.0, 0.0);
    let body = [70, 76, 70];
    m.rounded(&base, vec3(0.42, 0.16, 0.42), 0.12, body, 0.04, 2);
    m.rounded(
        &(base * Mat4::from_translation(vec3(0.0, 0.16, 0.0))),
        vec3(0.24, 0.08, 0.24),
        0.06,
        team_look(team).vest,
        0.03,
        2,
    );
    for k in 0..6 {
        let a = k as f32 * std::f32::consts::TAU / 6.0 + 0.52;
        let dir = vec3(a.sin(), 0.0, a.cos());
        let mid = base * Mat4::from_translation(dir * 0.65) * Mat4::from_rotation_y(a);
        m.boxed(&mid, vec3(0.04, 0.035, 0.38), body, 0.02);
        let tip = base * Mat4::from_translation(dir * 1.05 + vec3(0.0, 0.08, 0.0));
        m.boxed(&tip, vec3(0.07, 0.07, 0.07), METAL, 0.02);
        let spin = tip
            * Mat4::from_translation(vec3(0.0, 0.1, 0.0))
            * Mat4::from_rotation_y(t * 35.0 + k as f32);
        m.sphere(
            &(spin * Mat4::from_scale(vec3(1.0, 0.06, 1.0))),
            0.34,
            [200, 205, 212],
            0.0,
        );
    }
    for s in [-1.0f32, 1.0] {
        m.boxed(
            &(base * Mat4::from_translation(vec3(s * 0.3, -0.34, 0.0))),
            vec3(0.02, 0.2, 0.02),
            METAL,
            0.0,
        );
        m.boxed(
            &(base * Mat4::from_translation(vec3(s * 0.3, -0.54, 0.0))),
            vec3(0.03, 0.02, 0.3),
            METAL,
            0.0,
        );
    }
    for k in 0..ammo.min(6) {
        let x = (k as f32 - 2.5) * 0.11;
        let at = (base * Mat4::from_translation(vec3(x, -0.3, 0.0)))
            .w_axis
            .truncate();
        add_drop(m, at, 0.0);
    }
    m.ink = prev;
}

/// A bomb as dropped from the drone (nose down, with fins).
pub fn add_drop(m: &mut ToonMesh, pos: Vec3, t: f32) {
    let base = Mat4::from_translation(pos) * Mat4::from_rotation_y(t * 3.0);
    let olive = [96, 110, 66];
    m.sphere(
        &(base
            * Mat4::from_translation(vec3(0.0, -0.05, 0.0))
            * Mat4::from_scale(vec3(1.0, 1.4, 1.0))),
        0.07,
        olive,
        0.01,
    );
    m.boxed(
        &(base * Mat4::from_translation(vec3(0.0, 0.08, 0.0))),
        vec3(0.05, 0.06, 0.05),
        olive,
        0.01,
    );
    for r in [0.0f32, std::f32::consts::FRAC_PI_2] {
        m.boxed(
            &(base * Mat4::from_rotation_y(r) * Mat4::from_translation(vec3(0.0, 0.15, 0.0))),
            vec3(0.08, 0.04, 0.008),
            [60, 66, 50],
            0.0,
        );
    }
}

/// Ground robot (НРК): a small tracked chassis with a machine-gun turret.
/// `aim_pitch` tilts the gun.
pub fn add_nrk(
    m: &mut ToonMesh,
    pos: Vec3,
    yaw: f32,
    aim_pitch: f32,
    team: Team,
    t: f32,
    auto: bool,
) {
    let prev = m.ink.replace(team_ink(team));
    let base = trs(pos, yaw, 0.0, 0.0);
    let hull = [104, 112, 84];
    let dark = [44, 46, 50];
    for s in [-1.0f32, 1.0] {
        m.rounded(
            &(base * Mat4::from_translation(vec3(s * 0.42, 0.2, 0.0))),
            vec3(0.13, 0.2, 0.62),
            0.12,
            dark,
            0.03,
            2,
        );
        for k in 0..4 {
            let z = -0.42 + k as f32 * 0.28;
            m.sphere(
                &(base
                    * Mat4::from_translation(vec3(s * 0.56, 0.18, z))
                    * Mat4::from_rotation_x(t * 6.0)
                    * Mat4::from_scale(vec3(0.4, 1.0, 1.0))),
                0.11,
                [120, 124, 130],
                0.0,
            );
        }
    }
    m.rounded(
        &(base * Mat4::from_translation(vec3(0.0, 0.42, 0.0))),
        vec3(0.33, 0.16, 0.55),
        0.07,
        hull,
        0.03,
        2,
    );
    m.boxed(
        &(base * Mat4::from_translation(vec3(0.0, 0.5, 0.0))),
        vec3(0.335, 0.035, 0.4),
        team_look(team).vest,
        0.0,
    );
    let turret = base * Mat4::from_translation(vec3(0.0, 0.72, 0.05));
    m.rounded(&turret, vec3(0.2, 0.12, 0.22), 0.06, hull, 0.03, 2);
    let gun = turret
        * Mat4::from_translation(vec3(0.0, NRK_GUN_LOCAL_Y, -0.05))
        * Mat4::from_rotation_x(aim_pitch);
    m.boxed(
        &(gun * Mat4::from_translation(vec3(0.0, 0.0, -0.25))),
        vec3(0.07, 0.07, 0.2),
        dark,
        0.02,
    );
    m.boxed(
        &(gun * Mat4::from_translation(vec3(0.0, 0.0, -0.6))),
        vec3(0.025, 0.025, 0.2),
        dark,
        0.015,
    );
    m.boxed(
        &(gun * Mat4::from_translation(vec3(0.09, -0.04, -0.2))),
        vec3(0.05, 0.07, 0.09),
        [104, 120, 70],
        0.0,
    );
    m.boxed(
        &(turret * Mat4::from_translation(vec3(-0.13, 0.18, -0.1))),
        vec3(0.06, 0.05, 0.06),
        dark,
        0.015,
    );
    // an autonomous robot blinks red
    let led = if auto && (t * 3.0).fract() < 0.5 {
        [255, 80, 60]
    } else {
        [120, 200, 255]
    };
    m.fill.add_box_ex(
        &(turret * Mat4::from_translation(vec3(-0.13, 0.18, -0.165))),
        vec3(0.03, 0.025, 0.01),
        led,
        255,
        true,
    );
    m.boxed(
        &(base * Mat4::from_translation(vec3(0.22, 0.9, 0.4))),
        vec3(0.01, 0.35, 0.01),
        dark,
        0.0,
    );
    m.ink = prev;
}

/// Gun height above the turret ring, so the muzzle sits where the server shoots from.
const NRK_GUN_LOCAL_Y: f32 = toon_shared::vehicles::NRK_GUN_Y - 0.72;

/// Battle royale supplies on the ground: a medkit, an ammo box, a vest or a grenade crate.
/// A bright ring on the ground round the thing the use key would pick up.
pub fn add_pick_ring(m: &mut ToonMesh, pos: Vec3, t: f32) {
    let (r0, r1) = (0.42, 0.5 + 0.03 * (t * 5.0).sin());
    let n = 20;
    let y = pos.y + 0.03;
    for k in 0..n {
        let a0 = k as f32 / n as f32 * std::f32::consts::TAU;
        let a1 = (k + 1) as f32 / n as f32 * std::f32::consts::TAU;
        let p = |a: f32, r: f32| vec3(pos.x + a.cos() * r, y, pos.z + a.sin() * r);
        m.fill.add_quad(
            [p(a0, r0), p(a0, r1), p(a1, r1), p(a1, r0)],
            [255, 214, 90, 255],
        );
    }
}

pub fn add_loot(m: &mut ToonMesh, pos: Vec3, kind: toon_shared::protocol::LootKind, t: f32) {
    use toon_shared::protocol::LootKind;
    // a slow bob and turn so they catch the eye
    let base = trs(
        pos + vec3(0.0, 0.25 + (t * 2.0).sin() * 0.05, 0.0),
        t * 0.8,
        0.0,
        0.0,
    );
    match kind {
        LootKind::Medkit => {
            m.boxed(&base, vec3(0.3, 0.18, 0.22), [240, 240, 236], 0.02);
            let cross = [220, 40, 40];
            m.boxed(
                &(base * Mat4::from_translation(vec3(0.0, 0.181, 0.0))),
                vec3(0.16, 0.005, 0.05),
                cross,
                0.0,
            );
            m.boxed(
                &(base * Mat4::from_translation(vec3(0.0, 0.181, 0.0))),
                vec3(0.05, 0.005, 0.16),
                cross,
                0.0,
            );
        }
        LootKind::Ammo(a) => {
            use toon_shared::inventory::AmmoType;
            // the box's band tells the calibre
            let band = match a {
                AmmoType::Pistol => [220, 200, 120],
                AmmoType::Smg => [120, 200, 230],
                AmmoType::Rifle => [230, 150, 60],
                AmmoType::Shotgun => [220, 70, 60],
                AmmoType::Sniper => [170, 110, 220],
                AmmoType::Lmg => [120, 210, 110],
                AmmoType::Rocket => [200, 80, 40],
                AmmoType::Bolt => [150, 110, 60],
            };
            m.boxed(&base, vec3(0.3, 0.15, 0.2), [86, 100, 60], 0.02);
            m.boxed(
                &(base * Mat4::from_translation(vec3(0.0, 0.0, 0.0))),
                vec3(0.31, 0.05, 0.21),
                band,
                0.0,
            );
        }
        LootKind::Armor(l) => {
            let c = level_color(l);
            m.boxed(&base, vec3(0.3, 0.34, 0.1), c, 0.02);
            m.boxed(
                &(base * Mat4::from_translation(vec3(0.0, 0.05, 0.11))),
                vec3(0.2, 0.18, 0.02),
                toon_shared::map::shade(c, 1.2),
                0.0,
            );
        }
        LootKind::Helmet(l) => {
            let c = level_color(l);
            m.sphere(
                &(base * Mat4::from_scale(vec3(1.0, 0.75, 1.1))),
                0.24,
                c,
                0.02,
            );
            m.boxed(
                &(base * Mat4::from_translation(vec3(0.0, -0.06, 0.0))),
                vec3(0.27, 0.03, 0.29),
                toon_shared::map::shade(c, 0.8),
                0.0,
            );
        }
        LootKind::Backpack(l) => {
            let c = [[0, 0, 0], [110, 120, 80], [90, 110, 140], [70, 70, 80]][l.min(3) as usize];
            m.boxed(&base, vec3(0.24, 0.3, 0.14), c, 0.02);
            m.boxed(
                &(base * Mat4::from_translation(vec3(0.0, -0.08, 0.15))),
                vec3(0.18, 0.12, 0.05),
                toon_shared::map::shade(c, 0.85),
                0.0,
            );
            m.boxed(
                &(base * Mat4::from_translation(vec3(0.0, 0.32, 0.0))),
                vec3(0.18, 0.04, 0.12),
                level_color(l),
                0.0,
            );
        }
        LootKind::Sight(l) => {
            // the sight itself, a little bigger than on a gun, on a small olive case
            m.boxed(
                &(base * Mat4::from_translation(vec3(0.0, -0.08, 0.0))),
                vec3(0.2, 0.05, 0.14),
                [96, 104, 70],
                0.02,
            );
            let at = base * Mat4::from_scale(Vec3::splat(2.2));
            for p in sight_parts(l, 0.0) {
                m.boxed(&(at * Mat4::from_translation(p.c)), p.h, p.col, 0.01);
            }
        }
        LootKind::Mod(k) => {
            use toon_shared::inventory::{MOD_COMPENSATOR, MOD_EXTMAG, MOD_SUPPRESSOR};
            m.boxed(
                &(base * Mat4::from_translation(vec3(0.0, -0.08, 0.0))),
                vec3(0.2, 0.05, 0.14),
                [96, 104, 70],
                0.02,
            );
            let (h, c) = match k {
                MOD_SUPPRESSOR => (vec3(0.05, 0.05, 0.22), [36, 38, 42]),
                MOD_COMPENSATOR => (vec3(0.06, 0.06, 0.08), [80, 82, 88]),
                MOD_EXTMAG => (vec3(0.05, 0.16, 0.08), [50, 52, 58]),
                _ => (vec3(0.04, 0.12, 0.04), [42, 44, 50]),
            };
            m.boxed(
                &(base * Mat4::from_translation(vec3(0.0, 0.08, 0.0))),
                h,
                c,
                0.015,
            );
        }
        LootKind::Bandages => {
            m.boxed(&base, vec3(0.16, 0.08, 0.12), [246, 240, 230], 0.02);
            m.boxed(&base, vec3(0.17, 0.03, 0.13), [210, 60, 60], 0.0);
        }
        LootKind::Drink => {
            m.boxed(&base, vec3(0.07, 0.16, 0.07), [60, 200, 120], 0.02);
            m.boxed(
                &(base * Mat4::from_translation(vec3(0.0, 0.17, 0.0))),
                vec3(0.05, 0.02, 0.05),
                [200, 200, 205],
                0.0,
            );
        }
        LootKind::Molotov => add_molotov(m, pos + vec3(0.0, 0.1, 0.0), t),
        LootKind::Flash => {
            m.boxed(&base, vec3(0.07, 0.13, 0.07), [90, 96, 110], 0.02);
            m.boxed(
                &(base * Mat4::from_translation(vec3(0.0, 0.15, 0.0))),
                vec3(0.05, 0.03, 0.05),
                [210, 200, 80],
                0.0,
            );
        }
        LootKind::Grenades => {
            m.boxed(&base, vec3(0.3, 0.14, 0.24), [130, 100, 60], 0.02);
            for dx in [-0.12f32, 0.12] {
                m.sphere(
                    &(base * Mat4::from_translation(vec3(dx, 0.2, 0.0))),
                    0.09,
                    [70, 110, 60],
                    0.01,
                );
            }
        }
    }
}

/// A parachute canopy over a falling player.
pub fn add_parachute(m: &mut ToonMesh, pos: Vec3, yaw: f32, t: f32) {
    let sway = (t * 1.3).sin() * 0.06;
    let base = trs(pos + vec3(0.0, 4.6, 0.0), yaw, 0.0, sway);
    let colours = [[230, 90, 60], [250, 236, 210]];
    for k in 0..5 {
        let x = (k as f32 - 2.0) * 0.9;
        let y = -(k as f32 - 2.0).abs() * 0.25;
        m.boxed(
            &(base * Mat4::from_translation(vec3(x, y, 0.0))),
            vec3(0.46, 0.12, 1.1),
            colours[k % 2],
            0.03,
        );
    }
    // lines down to the harness
    for x in [-1.9f32, 1.9] {
        for z in [-0.9f32, 0.9] {
            let top = vec3(x, -0.4, z);
            let bottom = vec3(0.0, -3.0, 0.0);
            let mid = (top + bottom) * 0.5;
            let d = top - bottom;
            let rot = Quat::from_rotation_arc(Vec3::Y, d.normalize());
            m.fill.add_box_ex(
                &(base * Mat4::from_rotation_translation(rot, mid)),
                vec3(0.015, d.length() * 0.5, 0.015),
                [40, 40, 44],
                255,
                true,
            );
        }
    }
}

/// The battle royale transport plane (big, slow, propellers).
pub fn add_transport(m: &mut ToonMesh, pos: Vec3, yaw: f32, t: f32) {
    let base = trs(pos, yaw, 0.0, 0.0);
    let body = [176, 182, 168];
    let dark = [96, 104, 92];
    // fuselage along -z (forward)
    m.boxed(&base, vec3(2.2, 2.2, 13.0), body, 0.08);
    m.boxed(
        &(base * Mat4::from_translation(vec3(0.0, 0.4, -13.6))),
        vec3(1.6, 1.4, 1.2),
        dark,
        0.06,
    );
    // wings and tail
    m.boxed(
        &(base * Mat4::from_translation(vec3(0.0, 1.6, -2.0))),
        vec3(18.0, 0.3, 2.6),
        body,
        0.06,
    );
    m.boxed(
        &(base * Mat4::from_translation(vec3(0.0, 1.8, 11.5))),
        vec3(6.0, 0.25, 1.6),
        body,
        0.05,
    );
    m.boxed(
        &(base * Mat4::from_translation(vec3(0.0, 4.2, 11.8))),
        vec3(0.25, 2.4, 1.6),
        dark,
        0.05,
    );
    // engines with spinning propellers
    for x in [-7.0f32, -3.6, 3.6, 7.0] {
        let e = base * Mat4::from_translation(vec3(x, 1.0, -3.4));
        m.boxed(&e, vec3(0.7, 0.7, 1.6), dark, 0.04);
        let prop =
            e * Mat4::from_translation(vec3(0.0, 0.0, -1.7)) * Mat4::from_rotation_z(t * 25.0 + x);
        m.boxed(&prop, vec3(2.0, 0.15, 0.05), [40, 40, 40], 0.0);
    }
}

/// Colour of an equipment level: grey, blue, black with gold.
pub fn level_color(level: u8) -> [u8; 3] {
    match level {
        3 => [50, 50, 58],
        2 => [60, 96, 160],
        _ => [130, 136, 120],
    }
}

/// A battle royale jeep or boat (`hp` in per cent: a battered one looks it).
pub fn add_car(
    m: &mut ToonMesh,
    kind: toon_shared::cars::CarKind,
    s: &toon_shared::cars::CarState,
    hp: u8,
    t: f32,
) {
    use toon_shared::cars::CarKind;
    let rot = Mat4::from_rotation_y(s.yaw + std::f32::consts::PI);
    let worn = if hp < 35 { 0.7 } else { 1.0 };
    let mut part = |local: Vec3, half: Vec3, c: [u8; 3], outline: f32| {
        let at = s.to_world(local);
        let c = toon_shared::map::shade(c, worn);
        m.boxed(&(Mat4::from_translation(at) * rot), half, c, outline);
    };
    let o = 0.03;
    match kind {
        CarKind::Jeep => {
            let body = [104, 124, 72];
            let dark = [70, 84, 50];
            part(vec3(0.0, 0.62, -0.1), vec3(1.0, 0.3, 1.95), body, o);
            // the hood, a little higher in front
            part(vec3(0.0, 0.98, 1.3), vec3(0.95, 0.08, 0.6), body, o);
            part(vec3(0.0, 0.7, 2.0), vec3(0.9, 0.2, 0.08), dark, o);
            // windshield frame and roll bar
            part(vec3(0.0, 1.35, 0.72), vec3(0.95, 0.04, 0.04), dark, 0.0);
            part(vec3(-0.92, 1.15, 0.72), vec3(0.04, 0.25, 0.04), dark, 0.0);
            part(vec3(0.92, 1.15, 0.72), vec3(0.04, 0.25, 0.04), dark, 0.0);
            part(vec3(0.0, 1.6, -0.95), vec3(0.95, 0.04, 0.04), dark, 0.0);
            part(vec3(-0.92, 1.25, -0.95), vec3(0.04, 0.35, 0.04), dark, 0.0);
            part(vec3(0.92, 1.25, -0.95), vec3(0.04, 0.35, 0.04), dark, 0.0);
            // seats and the steering wheel (on the driver's side)
            for (x, z) in [(-0.45, 0.35), (0.45, 0.35), (-0.45, -0.85), (0.45, -0.85)] {
                part(
                    vec3(x, 1.0, z - 0.25),
                    vec3(0.3, 0.3, 0.06),
                    [60, 50, 40],
                    0.0,
                );
            }
            part(
                vec3(-0.45, 1.25, 0.62),
                vec3(0.18, 0.18, 0.03),
                [30, 30, 30],
                0.0,
            );
            // spare wheel and wheels (turning with the speed)
            part(
                vec3(0.0, 0.85, -2.1),
                vec3(0.32, 0.32, 0.12),
                [36, 36, 36],
                o,
            );
            let spin = (t * s.speed * 1.5).sin() * 0.02;
            for (x, z) in [(-1.05, 1.3), (1.05, 1.3), (-1.05, -1.35), (1.05, -1.35)] {
                part(
                    vec3(x, 0.4 + spin, z),
                    vec3(0.16, 0.38, 0.38),
                    [36, 36, 36],
                    o,
                );
            }
            // headlights
            part(
                vec3(-0.6, 0.85, 2.09),
                vec3(0.14, 0.1, 0.02),
                [250, 240, 170],
                0.0,
            );
            part(
                vec3(0.6, 0.85, 2.09),
                vec3(0.14, 0.1, 0.02),
                [250, 240, 170],
                0.0,
            );
        }
        CarKind::Boat => {
            let hull = [236, 236, 228];
            let stripe = [200, 60, 50];
            part(vec3(0.0, 0.15, -0.2), vec3(1.0, 0.32, 2.1), hull, o);
            part(vec3(0.0, 0.2, 2.15), vec3(0.6, 0.27, 0.35), hull, o);
            part(vec3(0.0, 0.3, 2.6), vec3(0.25, 0.18, 0.2), hull, o);
            part(vec3(0.0, 0.32, -0.2), vec3(1.02, 0.06, 2.12), stripe, 0.0);
            // seats, the console and the outboard motor
            for (x, z) in [(-0.45, 0.35), (0.45, 0.35), (-0.45, -0.85), (0.45, -0.85)] {
                part(
                    vec3(x, 0.55, z - 0.2),
                    vec3(0.3, 0.08, 0.25),
                    [70, 90, 120],
                    0.0,
                );
            }
            part(
                vec3(-0.45, 0.75, 0.75),
                vec3(0.25, 0.25, 0.12),
                [80, 80, 86],
                0.0,
            );
            part(
                vec3(0.0, 0.55, -2.45),
                vec3(0.22, 0.4, 0.22),
                [40, 40, 44],
                o,
            );
            // a wake when it moves
            if s.speed.abs() > 2.0 {
                let k = (s.speed.abs() / 16.0).min(1.0);
                part(
                    vec3(0.0, -0.1, -2.9),
                    vec3(0.6 * k, 0.04, 0.5 * k),
                    [240, 250, 255],
                    0.0,
                );
            }
        }
    }
}

/// A supply crate (under its parachute while it falls).
pub fn add_airdrop(m: &mut ToonMesh, pos: Vec3, landed: bool, t: f32) {
    let base = Mat4::from_translation(pos + vec3(0.0, 0.7, 0.0));
    m.boxed(&base, vec3(0.75, 0.7, 0.75), [60, 110, 170], 0.04);
    for (dx, dz) in [(0.0, 0.76), (0.0, -0.76), (0.76, 0.0), (-0.76, 0.0)] {
        m.boxed(
            &(base * Mat4::from_translation(vec3(dx, 0.0, dz))),
            vec3(
                if dx == 0.0 { 0.78 } else { 0.02 },
                0.16,
                if dz == 0.0 { 0.78 } else { 0.02 },
            ),
            [240, 150, 40],
            0.0,
        );
    }
    if !landed {
        add_parachute(m, pos - vec3(0.0, 1.0, 0.0), t * 0.2, t);
    }
}

/// A death box: a wooden crate with what somebody carried.
pub fn add_deathbox(m: &mut ToonMesh, pos: Vec3) {
    let base = Mat4::from_translation(pos + vec3(0.0, 0.35, 0.0));
    m.boxed(&base, vec3(0.55, 0.35, 0.4), [120, 84, 52], 0.03);
    m.boxed(
        &(base * Mat4::from_translation(vec3(0.0, 0.37, 0.0))),
        vec3(0.58, 0.05, 0.43),
        [92, 62, 38],
        0.0,
    );
    for x in [-0.4f32, 0.4] {
        m.boxed(
            &(base * Mat4::from_translation(vec3(x, 0.0, 0.0))),
            vec3(0.04, 0.36, 0.41),
            [80, 54, 34],
            0.0,
        );
    }
}

/// A molotov: a bottle with a burning rag.
pub fn add_molotov(m: &mut ToonMesh, pos: Vec3, t: f32) {
    let base = trs(pos + vec3(0.0, 0.15, 0.0), t * 3.0, 0.0, 0.0);
    m.boxed(&base, vec3(0.07, 0.13, 0.07), [90, 150, 70], 0.02);
    m.boxed(
        &(base * Mat4::from_translation(vec3(0.0, 0.18, 0.0))),
        vec3(0.03, 0.06, 0.03),
        [90, 150, 70],
        0.0,
    );
    m.boxed(
        &(base * Mat4::from_translation(vec3(0.0, 0.27, 0.0))),
        vec3(0.035, 0.04, 0.035),
        [250, 150, 40],
        0.0,
    );
}
