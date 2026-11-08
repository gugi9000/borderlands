use std::collections::VecDeque;

const FNV_OFFSET: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedMap {
    pub name: String,
    pub size: usize,
    pub seed: u64,
    pub text: String,
}

impl GeneratedMap {
    pub fn file_name(&self) -> String {
        format!("{}-{}.map", self.name, self.size)
    }
}

pub fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(FNV_OFFSET, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(FNV_PRIME)
    })
}

pub fn generate(size: usize, name: &str) -> Result<GeneratedMap, String> {
    if !(11..=101).contains(&size) || size % 2 == 0 {
        return Err("map size must be an odd number from 11 to 101".to_string());
    }
    let name = name.trim().to_lowercase();
    if name.is_empty()
        || !name
            .chars()
            .all(|ch| ch.is_alphanumeric() || matches!(ch, '-' | '_' | ' '))
    {
        return Err(
            "map name must contain letters, numbers, spaces, hyphens or underscores only"
                .to_string(),
        );
    }
    let name_hash = fnv1a(name.as_bytes());
    let seed = (size as u64)
        .to_le_bytes()
        .iter()
        .fold(name_hash, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(FNV_PRIME)
        });
    let mut random = DeterministicRandom(seed);
    let half = size / 2;
    let quadrant_size = half + 1;
    let base_x = (random.next() % half as u64) as usize;
    let base_y = (random.next() % half as u64) as usize;
    let base = base_y * quadrant_size + base_x;
    let center = half * quadrant_size + half;
    let mut walls = vec![false; quadrant_size * quadrant_size];
    let mut candidates: Vec<_> = (0..walls.len())
        .filter(|&index| index != base && index != center)
        .collect();
    for index in (1..candidates.len()).rev() {
        let other = (random.next() % (index + 1) as u64) as usize;
        candidates.swap(index, other);
    }
    let target_walls = size * size / 5;
    let mut wall_count = 0;
    for candidate in candidates {
        let x = candidate % quadrant_size;
        let y = candidate / quadrant_size;
        let copies = if x == half { 1 } else { 2 } * if y == half { 1 } else { 2 };
        if wall_count + copies > target_walls {
            continue;
        }
        walls[candidate] = true;
        if connected(&walls, quadrant_size, base) {
            wall_count += copies;
        } else {
            walls[candidate] = false;
        }
        if wall_count == target_walls {
            break;
        }
    }

    let mut text = format!("; {name} ({size}x{size}), seed {seed:016x}\n");
    for y in 0..size {
        for x in 0..size {
            let source_x = x.min(size - 1 - x);
            let source_y = y.min(size - 1 - y);
            let source = source_y * quadrant_size + source_x;
            let tile = if source == base {
                match (x > half, y > half) {
                    (false, false) => 'A',
                    (true, false) => 'B',
                    (false, true) => 'C',
                    (true, true) => 'D',
                }
            } else if walls[source] {
                '#'
            } else {
                '.'
            };
            text.push(tile);
        }
        text.push('\n');
    }
    Ok(GeneratedMap {
        name,
        size,
        seed,
        text,
    })
}

fn connected(walls: &[bool], width: usize, start: usize) -> bool {
    let mut seen = vec![false; walls.len()];
    let mut queue = VecDeque::from([start]);
    seen[start] = true;
    let mut reached = 0;
    while let Some(index) = queue.pop_front() {
        reached += 1;
        let x = index % width;
        let y = index / width;
        for neighbor in [
            (x > 0).then(|| index - 1),
            (x + 1 < width).then_some(index + 1),
            (y > 0).then(|| index - width),
            (y + 1 < width).then_some(index + width),
        ]
        .into_iter()
        .flatten()
        {
            if !walls[neighbor] && !seen[neighbor] {
                seen[neighbor] = true;
                queue.push_back(neighbor);
            }
        }
    }
    reached == walls.iter().filter(|&&wall| !wall).count()
}

struct DeterministicRandom(u64);

impl DeterministicRandom {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);
        let mut value = self.0;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
        value ^ (value >> 31)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv1a_matches_standard_64_bit_vectors() {
        assert_eq!(fnv1a(b""), 0xcbf29ce484222325);
        assert_eq!(fnv1a(b"a"), 0xaf63dc4c8601ec8c);
        assert_eq!(fnv1a(b"foobar"), 0x85944171f73967e8);
    }

    #[test]
    fn names_are_lowercased_and_generation_is_reproducible() {
        let first = generate(35, "Grænseland").unwrap();
        assert_eq!(first, generate(35, "GRÆNSELAND").unwrap());
        assert_eq!(first, generate(35, " grænseland ").unwrap());
        assert_eq!(first.file_name(), "grænseland-35.map");
        assert_ne!(first.seed, generate(35, "another").unwrap().seed);
        assert_ne!(first.seed, generate(37, "Grænseland").unwrap().seed);
        let terrain = |map: GeneratedMap| map.text.lines().skip(1).collect::<String>();
        assert_ne!(terrain(first), terrain(generate(35, "another").unwrap()));
    }

    #[test]
    fn invalid_sizes_and_unsafe_names_are_rejected() {
        for size in [0, 10, 12, 100, 102, usize::MAX] {
            assert!(generate(size, "test").is_err());
        }
        for name in ["", " ", "..", "..\\outside", "a/b", "a:b", "a\nb"] {
            assert!(generate(11, name).is_err());
        }
    }

    #[test]
    fn maps_are_symmetric_connected_and_have_twenty_percent_walls() {
        for size in (11..=101).step_by(2) {
            for name in ["test", "æøåひらがな"] {
                let map = generate(size, name).unwrap();
                let rows: Vec<Vec<char>> = map
                    .text
                    .lines()
                    .filter(|line| !line.starts_with(';'))
                    .map(|line| line.chars().collect())
                    .collect();
                assert_eq!(rows.len(), size);
                assert!(rows.iter().all(|row| row.len() == size));
                let mut bases = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
                let mut walls = Vec::new();
                for (y, row) in rows.iter().enumerate() {
                    for (x, &tile) in row.iter().enumerate() {
                        assert!(matches!(tile, '.' | '#' | 'A'..='D'));
                        if ('A'..='D').contains(&tile) {
                            bases[(tile as u8 - b'A') as usize].push((x, y));
                        }
                        walls.push(tile == '#');
                        assert_eq!(tile == '#', rows[y][size - 1 - x] == '#');
                        assert_eq!(tile == '#', rows[size - 1 - y][x] == '#');
                        assert_eq!(tile == '#', rows[size - 1 - y][size - 1 - x] == '#');
                    }
                }
                assert!(bases.iter().all(|positions| positions.len() == 1));
                let (x, y) = bases[0][0];
                assert!(x < size / 2 && y < size / 2);
                assert_eq!(bases[1][0], (size - 1 - x, y));
                assert_eq!(bases[2][0], (x, size - 1 - y));
                assert_eq!(bases[3][0], (size - 1 - x, size - 1 - y));
                assert!(connected(&walls, size, y * size + x));
                let ratio = walls.iter().filter(|&&wall| wall).count() as f64 / walls.len() as f64;
                assert!((0.18..=0.21).contains(&ratio), "{size}, {name}: {ratio}");
            }
        }
    }
}
