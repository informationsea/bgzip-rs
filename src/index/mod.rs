pub mod tbi;

use std::cmp::{max, min};
use std::io;

pub const DEFAULT_MIN_SHIFT: u32 = 14;
pub const DEFAULT_DEPTH: u32 = 5;

pub trait IndexedFile {
    fn fetch0(&mut self, rid: u32, begin: u64, end: u64) -> io::Result<()>;

    // 1-based close-close
    fn fetch(&mut self, rid: u32, begin: u64, end: u64) -> io::Result<()> {
        self.fetch0(rid, begin - 1, end)
    }

    fn read(&mut self, data: &mut Vec<u8>) -> io::Result<Option<(u64, u64)>>;

    fn read_all(&mut self) -> io::Result<Vec<(u64, u64, Vec<u8>)>> {
        let mut data = Vec::new();
        loop {
            let mut one = Vec::new();
            let result = self.read(&mut one)?;
            if let Some((start, end)) = result {
                data.push((start, end, one));
            } else {
                break;
            }
        }
        Ok(data)
    }
}

pub trait Index {
    fn region_chunks(&self, rid: u32, begin: u64, end: u64) -> Vec<(u64, u64)>;
    fn rid2name(&self, rid: u32) -> &[u8];
    fn name2rid(&self, name: &[u8]) -> u32;
    fn names(&self) -> &[Vec<u8>];
}

/* calculate bin given an alignment covering [beg,end) (zero-based, half-close-half-open) */
pub fn reg2bin(beg: u64, mut end: u64, min_shift: u32, depth: u32) -> u32 {
    let mut l = depth;
    let mut s = min_shift;
    let mut t = ((1 << depth * 3) - 1) / 7;
    end -= 1;

    while l > 0 {
        if beg >> s == end >> s {
            return (t + (beg >> s)) as u32;
        }

        l -= 1;
        s += 3;
        t -= 1 << l * 3;
    }
    0
}
/* calculate the list of bins that may overlap with region [beg,end) (zero-based) */
pub fn reg2bins(beg: u64, mut end: u64, min_shift: u32, depth: u32, bins: &mut Vec<u16>) {
    let mut l = 0;
    let mut t = 0;

    let mut s = min_shift + depth * 3;
    end -= 1;
    while l <= depth {
        let b = t + (beg >> s);
        let e = t + (end >> s);
        for i in b..(e + 1) {
            bins.push(i as u16);
        }

        s -= 3;
        t += 1 << l * 3;
        l += 1;
    }
}

pub(crate) struct RegionSimplify<T: PartialEq + PartialOrd + Ord + Copy> {
    regions: Vec<(T, T)>,
}

impl<T: PartialEq + PartialOrd + Ord + Copy> RegionSimplify<T> {
    pub(crate) fn new() -> RegionSimplify<T> {
        RegionSimplify {
            regions: Vec::new(),
        }
    }

    pub(crate) fn regions(self) -> Vec<(T, T)> {
        self.regions
    }

    pub(crate) fn insert(&mut self, start: T, end: T) {
        if self.regions.len() == 0 {
            self.regions.push((start, end));
            return;
        }

        let overlapped: Vec<(usize, (T, T))> = (&self.regions)
            .into_iter()
            .enumerate()
            .filter(|(_, (x, y))| *x <= end && start <= *y)
            .map(|(i, (x, y))| (i, (*x, *y)))
            .collect();
        if overlapped.len() > 0 {
            for (i, (_, _)) in (&overlapped).into_iter().rev() {
                self.regions.remove(*i);
            }
            let new_start = (&overlapped)
                .into_iter()
                .fold(start, |x, (_, (y, _))| min(x, *y));
            let new_end = (&overlapped)
                .into_iter()
                .fold(end, |x, (_, (_, y))| max(x, *y));
            self.regions.push((new_start, new_end));
        } else {
            self.regions.push((start, end));
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_region_simplify() {
        let mut region_simplify = super::RegionSimplify::new();
        region_simplify.insert(10, 20);
        region_simplify.insert(20, 30);
        assert_eq!(vec![(10, 30)], region_simplify.regions());

        let mut region_simplify = super::RegionSimplify::new();
        region_simplify.insert(10, 20);
        region_simplify.insert(20, 30);
        region_simplify.insert(50, 60);
        assert_eq!(vec![(10, 30), (50, 60)], region_simplify.regions());

        let mut region_simplify = super::RegionSimplify::new();
        region_simplify.insert(10, 20);
        region_simplify.insert(20, 30);
        region_simplify.insert(50, 60);
        region_simplify.insert(0, 5);
        assert_eq!(vec![(10, 30), (50, 60), (0, 5)], region_simplify.regions());

        let mut region_simplify = super::RegionSimplify::new();
        region_simplify.insert(10, 20);
        region_simplify.insert(20, 30);
        region_simplify.insert(50, 60);
        region_simplify.insert(0, 5);
        region_simplify.insert(5, 10);
        assert_eq!(vec![(50, 60), (0, 30)], region_simplify.regions());

        let mut region_simplify = super::RegionSimplify::new();
        region_simplify.insert(10, 20);
        region_simplify.insert(20, 30);
        region_simplify.insert(50, 60);
        region_simplify.insert(0, 5);
        region_simplify.insert(5, 10);
        region_simplify.insert(40, 45);
        assert_eq!(vec![(50, 60), (0, 30), (40, 45)], region_simplify.regions());

        let mut region_simplify = super::RegionSimplify::new();
        region_simplify.insert(10, 20);
        region_simplify.insert(20, 30);
        region_simplify.insert(50, 60);
        region_simplify.insert(0, 5);
        region_simplify.insert(5, 10);
        region_simplify.insert(40, 45);
        region_simplify.insert(30, 50);
        assert_eq!(vec![(0, 60)], region_simplify.regions());
    }

    use std::io;
    use std::io::prelude::*;

    #[test]
    fn test_reg2bin() {
        let expected_data = include_bytes!("../../testfiles/index-c/bintest_case.txt");
        let mut expected_reader = io::BufReader::new(&expected_data[..]);
        let mut line = String::new();
        loop {
            line.clear();
            if expected_reader.read_line(&mut line).unwrap() == 0 {
                break;
            }
            let elements: Vec<&str> = line.split('\t').collect();
            let start = elements[0].parse::<u64>().unwrap();
            let end = elements[1].parse::<u64>().unwrap();
            let expected_bin = elements[2].parse::<u32>().unwrap();
            let expected_bins: Vec<u16> = elements
                .into_iter()
                .skip(3)
                .map(|x| x.trim().parse::<u16>().unwrap())
                .collect();
            assert_eq!(
                expected_bin,
                super::reg2bin(start, end, super::DEFAULT_MIN_SHIFT, super::DEFAULT_DEPTH)
            );

            let mut bins = Vec::new();
            super::reg2bins(
                start,
                end,
                super::DEFAULT_MIN_SHIFT,
                super::DEFAULT_DEPTH,
                &mut bins,
            );
            assert_eq!(expected_bins, bins);
        }
    }
}
