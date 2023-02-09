use std::convert::TryInto;

/// calculate bin given an alignment covering [beg,end) (zero-based, half-close-half-open)
pub fn reg2bin(beg: i64, end: i64, min_shift: u32, depth: u32) -> u32 {
    let end = end - 1;
    let mut s = min_shift;
    let mut t = ((1 << (depth * 3)) - 1) / 7;

    for l2 in 0..depth {
        //eprintln!("depth: {}", l2);
        let l = depth - l2;
        if beg >> s == end >> s {
            //eprintln!("value: {}", (t + (beg >> s)));
            return (t + (beg >> s)).try_into().unwrap();
        };
        s += 3;
        //let t2 = t;
        t -= 1 << ((l - 1) * 3);
        //eprintln!("t : {} -> {} / {} / {}", t2, t, l, 1 << (l * 3));
    }

    0
}

/// calculate the list of bins that may overlap with region [beg,end) (zero-based)
pub fn reg2bins(beg: i64, end: i64, min_shift: u32, depth: u32) -> Vec<u32> {
    let mut bins: Vec<u32> = Vec::new();
    let end = end - 1;
    let mut s = min_shift + depth * 3;
    let mut t = 0;

    for l in 0..=depth {
        let b = t + (beg >> s);
        let e = t + (end >> s);
        for i in b..=e {
            bins.push(i.try_into().unwrap());
        }
        s -= 3;
        t += 1 << (l * 3);
    }

    bins
}
