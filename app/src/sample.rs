//! A made-up data file for trying the program: nothing in it comes from a real car.
use crate::model::DataOrg;

fn put(d: &mut [u8], p: &mut usize, v: f64) {
    let b = DataOrg::HiLo.encode(v, false);
    d[*p..*p + 2].copy_from_slice(&b);
    *p += 2;
}

fn table(d: &mut [u8], at: usize, nx: usize, ny: usize, f: &dyn Fn(usize, usize) -> f64) -> usize {
    let mut p = at;
    put(d, &mut p, nx as f64);
    put(d, &mut p, ny as f64);
    for i in 0..nx {
        put(d, &mut p, 1000.0 + i as f64 * 400.0);
    }
    for j in 0..ny {
        put(d, &mut p, 100.0 + j as f64 * 150.0);
    }
    for j in 0..ny {
        for i in 0..nx {
            put(d, &mut p, f(i, j));
        }
    }
    p
}

/// 256 KB: empty areas, code-like noise, a text and several tables with and without axes.
pub fn sample_data() -> Vec<u8> {
    let size = 0x40000;
    let mut x = 0x2545F491u32;
    let mut d: Vec<u8> = (0..size)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            x as u8
        })
        .collect();
    d[..0x2000].fill(0xFF);
    d[0x38000..].fill(0xFF);
    d[0x20000..0x24000].fill(0);
    let t = b"0281010000 1037360000 SAMPLE DATA, NOT FROM A REAL CAR";
    d[0x2100..0x2100 + t.len()].copy_from_slice(t);
    let p = table(&mut d, 0x8000, 16, 12, &|i, j| 2000.0 + (i * j) as f64 * 18.0 + 200.0 * (i as f64 / 3.0).sin());
    let p = table(&mut d, p + 64, 10, 8, &|i, j| 5000.0 + i as f64 * 90.0 - j as f64 * 120.0);
    let _ = p;
    table(&mut d, 0x9000, 12, 16, &|i, j| 800.0 + i as f64 * 50.0 + (j * j) as f64 * 7.0);
    table(&mut d, 0xA000, 8, 8, &|i, j| 300.0 + (i * i) as f64 * 3.0 + j as f64 * 11.0);
    let mut o = 0xB000;
    for j in 0..10 {
        for i in 0..14 {
            put(&mut d, &mut o, 3000.0 + i as f64 * 120.0 + j as f64 * 40.0 + 50.0 * (i as f64).sin());
        }
    }
    let mut o = 0xC000;
    for j in 0..20 {
        for i in 0..10 {
            d[o] = (40 + i * 12 + j * 3).min(255) as u8;
            o += 1;
        }
    }
    d
}
