

pub trait Encode {
    fn write(&self, a: &mut u64) -> String; 
}


#[cfg(test)]
mod tests {
    // Note this useful idiom: importing names from outer (for mod tests) scope.
    use super::*;

    #[test]
    fn test_href() {
    }
}
