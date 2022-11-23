/// Searches for an item with a given pattern, then mutates it.
/// If it doesn't find the pattern, uses a default value.
macro_rules! map_item_pattern {
    { $self:expr => $pat:pat => $default:expr => $($block:tt)* } => {
        let target = if let Some(target) = $self.iter().find(|t| match t {
            $pat => true,
            _ => false,
        }) {
            target
        } else {
            $default;
        };
        if let $pat = target {
            $($block)*
        } else {
            panic!("Prob our default value doesn't satisfy pattern.");
        }
    };
}

/// Searches for an item with a given pattern, then mutates it.
/// If it doesn't find the pattern, creates and adds a default value, applying mutation to it.
macro_rules! mutate_item_pattern {
    ($self:expr => $pat:pat => $default:expr, $mapping:expr) => {
        let target = if let Some(target) = $self.iter_mut().find(|t| match t {
            $pat => true,
            _ => false,
        }) {
            target
        } else {
            let default = $default;
            $self.insert($self.len(), default);
            $self.iter_mut().last().unwrap()
        };
        if let $pat = target {
            $mapping
        } else {
            panic!("Prob our default value doesn't satisfy pattern.");
        }
    };
}

/// Searches for an item matching given pattern
#[macro_export]
macro_rules! find_pattern {
    ($self:expr => $pat:pat) => {
        $self.iter().find(|t| match t {
            $pat => true,
            _ => false,
        })
    };
}
