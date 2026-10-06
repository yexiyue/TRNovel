//! Upstream's monotonic timestamp repair (longest nondecreasing subsequence).
pub(super) fn repair(data: &[u64]) -> Vec<u64> {
    if data.is_empty() {
        return Vec::new();
    }
    let n = data.len();
    let mut dp = vec![1; n];
    let mut parent = vec![None; n];
    for i in 1..n {
        for j in 0..i {
            if data[j] <= data[i] && dp[j] + 1 > dp[i] {
                dp[i] = dp[j] + 1;
                parent[i] = Some(j);
            }
        }
    }
    let longest = *dp.iter().max().unwrap();
    let mut current = Some(dp.iter().position(|value| *value == longest).unwrap());
    let mut normal = vec![false; n];
    while let Some(i) = current {
        normal[i] = true;
        current = parent[i];
    }
    let mut result = data.to_vec();
    let mut i = 0;
    while i < n {
        if normal[i] {
            i += 1;
            continue;
        }
        let mut j = i;
        while j < n && !normal[j] {
            j += 1;
        }
        let left = (0..i).rev().find(|k| normal[*k]);
        let right = (j..n).find(|k| normal[*k]);
        for k in i..j {
            result[k] = match (left, right) {
                (Some(left), Some(right)) if j - i <= 2 => {
                    if k - left <= right - k {
                        result[left]
                    } else {
                        result[right]
                    }
                }
                (Some(left), Some(right)) => {
                    result[left]
                        + (result[right] - result[left]) * (k - i + 1) as u64 / (j - i + 1) as u64
                }
                (Some(left), None) => result[left],
                (None, Some(right)) => result[right],
                (None, None) => data[k],
            };
        }
        i = j;
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn monotonic_and_repair_match_reference() {
        assert_eq!(repair(&[0, 80, 160]), vec![0, 80, 160]);
        assert_eq!(repair(&[0, 80, 40, 240]), vec![0, 80, 80, 240]);
        assert!(
            repair(&[100, 40, 30, 20, 400])
                .windows(2)
                .all(|p| p[0] <= p[1])
        );
    }
}
