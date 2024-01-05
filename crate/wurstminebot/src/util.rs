use {
    std::fmt,
    itertools::Itertools as _,
};

pub(crate) fn join<T: fmt::Display, I: IntoIterator<Item = T>>(words: I) -> Option<String> {
    let mut words = words.into_iter().collect_vec();
    match &*words {
        [] => None,
        [word] => Some(word.to_string()),
        [left, right] => Some(format!("{} and {}", left, right)),
        _ => {
            let last = words.pop().expect("match checks that words is not empty");
            Some(format!("{}, and {}", words.into_iter().join(", "), last))
        }
    }
}
