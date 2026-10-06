use crate::{SubGraphDefinition, SubGraphLibrary};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct LibraryManager {
    libraries: HashMap<String, SubGraphLibrary>,
    subgraph_cache: HashMap<String, SubGraphDefinition>,
    search_paths: Vec<std::path::PathBuf>,
}

impl LibraryManager {
    pub fn new() -> Self {
        Self {
            libraries: HashMap::new(),
            subgraph_cache: HashMap::new(),
            search_paths: Vec::new(),
        }
    }

    pub fn add_search_path(&mut self, path: impl Into<std::path::PathBuf>) {
        self.search_paths.push(path.into());
    }

    pub fn load_all_libraries(
        &mut self,
        mut load: impl FnMut(
            &std::path::Path,
        ) -> Result<Vec<SubGraphLibrary>, Box<dyn std::error::Error>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        for search_path in self.search_paths.clone() {
            for library in load(&search_path)? {
                self.register_library(library);
            }
        }
        Ok(())
    }

    pub fn register_library(&mut self, library: SubGraphLibrary) {
        for subgraph in &library.subgraphs {
            self.subgraph_cache
                .insert(subgraph.id.clone(), subgraph.clone());
        }
        self.libraries.insert(library.id.clone(), library);
    }

    pub fn get_subgraph(&self, id: &str) -> Option<&SubGraphDefinition> {
        self.subgraph_cache.get(id)
    }

    pub fn get_libraries(&self) -> &HashMap<String, SubGraphLibrary> {
        &self.libraries
    }

    pub fn get_all_subgraphs(&self) -> Vec<&SubGraphDefinition> {
        self.subgraph_cache.values().collect()
    }

    pub fn get_subgraphs_by_category(&self, category: &str) -> Vec<&SubGraphDefinition> {
        self.libraries
            .values()
            .filter(|lib| lib.category == category)
            .flat_map(|lib| lib.subgraphs.iter())
            .collect()
    }

    pub fn default_stdlib_path() -> std::path::PathBuf {
        std::path::PathBuf::from("libraries").join("std")
    }

    pub fn default_user_library_path() -> std::path::PathBuf {
        std::env::current_dir()
            .unwrap_or_default()
            .join("libraries")
            .join("user")
    }
}

impl Default for LibraryManager {
    fn default() -> Self {
        let mut manager = Self::new();
        manager.add_search_path(Self::default_stdlib_path());
        manager.add_search_path(Self::default_user_library_path());
        manager
    }
}
