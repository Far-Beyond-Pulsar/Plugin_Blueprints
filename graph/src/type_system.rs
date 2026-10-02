use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TypeInfo {
    pub base_type: String,
    pub wrappers: Vec<WrapperType>,
    pub is_wildcard: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum WrapperType {
    Vec,
    HashMap,
    HashSet,
    Arc,
    Box,
    Ref,
    RefMut,
    Option,
    Result,
}

impl TypeInfo {
    /// Parse a Rust type string into TypeInfo
    pub fn parse(type_str: &str) -> Self {
        if type_str == "?" || type_str == "_" || type_str == "T" {
            return Self {
                base_type: "wildcard".to_string(),
                wrappers: Vec::new(),
                is_wildcard: true,
            };
        }

        let mut remaining = type_str;
        let mut wrappers = Vec::new();

        // Parse wrapper types from outside in
        loop {
            remaining = remaining.trim();

            if let Some(inner) = Self::extract_wrapper(remaining, "Vec<", ">") {
                wrappers.push(WrapperType::Vec);
                remaining = inner;
            } else if let Some(inner) = Self::extract_wrapper(remaining, "HashMap<", ">") {
                wrappers.push(WrapperType::HashMap);
                remaining = inner;
            } else if let Some(inner) = Self::extract_wrapper(remaining, "HashSet<", ">") {
                wrappers.push(WrapperType::HashSet);
                remaining = inner;
            } else if let Some(inner) = Self::extract_wrapper(remaining, "Arc<", ">") {
                wrappers.push(WrapperType::Arc);
                remaining = inner;
            } else if let Some(inner) = Self::extract_wrapper(remaining, "Box<", ">") {
                wrappers.push(WrapperType::Box);
                remaining = inner;
            } else if let Some(inner) = Self::extract_wrapper(remaining, "&mut ", "") {
                wrappers.push(WrapperType::RefMut);
                remaining = inner;
            } else if let Some(inner) = Self::extract_wrapper(remaining, "&", "") {
                wrappers.push(WrapperType::Ref);
                remaining = inner;
            } else if let Some(inner) = Self::extract_wrapper(remaining, "Option<", ">") {
                wrappers.push(WrapperType::Option);
                remaining = inner;
            } else if let Some(inner) = Self::extract_wrapper(remaining, "Result<", ">") {
                wrappers.push(WrapperType::Result);
                remaining = inner;
            } else {
                break;
            }
        }

        // Check if the inner type is a wildcard
        let is_wildcard = remaining == "?"
            || remaining == "_"
            || remaining == "T"
            || remaining.len() == 1 && remaining.chars().next().unwrap().is_uppercase();

        Self {
            base_type: if is_wildcard {
                "wildcard".to_string()
            } else {
                remaining.to_string()
            },
            wrappers,
            is_wildcard,
        }
    }

    fn extract_wrapper<'a>(type_str: &'a str, prefix: &str, suffix: &str) -> Option<&'a str> {
        if type_str.starts_with(prefix) {
            if suffix.is_empty() {
                Some(&type_str[prefix.len()..])
            } else if type_str.ends_with(suffix) {
                Some(&type_str[prefix.len()..type_str.len() - suffix.len()])
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Check if two types are compatible for connection
    pub fn is_compatible_with(&self, other: &TypeInfo) -> bool {
        // Wildcard types can connect to anything
        if self.is_wildcard || other.is_wildcard {
            return true;
        }

        // For now, exact type match (can be extended later)
        self.base_type == other.base_type && self.wrappers == other.wrappers
    }

    /// Check if this type can be converted to another type
    pub fn can_convert_to(&self, other: &TypeInfo) -> bool {
        // Wildcard types can convert to anything
        if self.is_wildcard || other.is_wildcard {
            return true;
        }

        // Same base type with compatible wrappers
        if self.base_type == other.base_type {
            // Allow some wrapper conversions (e.g., T -> &T, T -> Box<T>)
            return true;
        }

        // Built-in conversions
        match (self.base_type.as_str(), other.base_type.as_str()) {
            ("i32", "f32") | ("i32", "f64") | ("f32", "f64") => true,
            ("&str", "String") | ("String", "&str") => true,
            _ => false,
        }
    }


}

impl std::fmt::Display for TypeInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_wildcard {
            return write!(f, "?");
        }

        let mut result = self.base_type.clone();

        // Apply wrappers from innermost to outermost (reverse order)
        for wrapper in self.wrappers.iter().rev() {
            result = match wrapper {
                WrapperType::Vec => format!("Vec<{}>", result),
                WrapperType::HashMap => format!("HashMap<{}>", result),
                WrapperType::HashSet => format!("HashSet<{}>", result),
                WrapperType::Arc => format!("Arc<{}>", result),
                WrapperType::Box => format!("Box<{}>", result),
                WrapperType::Ref => format!("&{}", result),
                WrapperType::RefMut => format!("&mut {}", result),
                WrapperType::Option => format!("Option<{}>", result),
                WrapperType::Result => format!("Result<{}>", result),
            };
        }

        write!(f, "{}", result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_parsing() {
        // Basic types
        assert_eq!(TypeInfo::parse("i32").base_type, "i32");
        assert_eq!(TypeInfo::parse("String").base_type, "String");

        // Wrapper types
        let vec_type = TypeInfo::parse("Vec<i32>");
        assert_eq!(vec_type.base_type, "i32");
        assert_eq!(vec_type.wrappers, vec![WrapperType::Vec]);

        // Nested wrappers
        let complex_type = TypeInfo::parse("Arc<Vec<String>>");
        assert_eq!(complex_type.base_type, "String");
        assert_eq!(
            complex_type.wrappers,
            vec![WrapperType::Arc, WrapperType::Vec]
        );

        // Wildcard types
        let wildcard = TypeInfo::parse("?");
        assert!(wildcard.is_wildcard);
        assert_eq!(wildcard.base_type, "wildcard");
    }

    #[test]
    fn test_type_compatibility() {
        let i32_type = TypeInfo::parse("i32");
        let f32_type = TypeInfo::parse("f32");
        let wildcard = TypeInfo::parse("?");

        // Wildcard compatibility
        assert!(wildcard.is_compatible_with(&i32_type));
        assert!(i32_type.is_compatible_with(&wildcard));

        // Same type compatibility
        assert!(i32_type.is_compatible_with(&i32_type));

        // Different type incompatibility (for now)
        assert!(!i32_type.is_compatible_with(&f32_type));
    }

    #[test]
    fn test_display() {
        assert_eq!(TypeInfo::parse("i32").to_string(), "i32");
        assert_eq!(TypeInfo::parse("Vec<i32>").to_string(), "Vec<i32>");
        assert_eq!(
            TypeInfo::parse("Arc<Vec<String>>").to_string(),
            "Arc<Vec<String>>"
        );
        assert_eq!(TypeInfo::parse("?").to_string(), "?");
    }
}
