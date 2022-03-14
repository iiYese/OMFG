use std::{
    io::{self, prelude::*},
    fs::{create_dir, File, rename},
    path::PathBuf
};

use anyhow::{*, Result as AnyHow};
use serde_json::{
    to_string, to_string_pretty,
    from_str
};

use crate::struct_diff::{StructDiff, Structure};
use crate::utils::*;

//avoid typo errors
const MODS: &str = "mods";
const SUPER_MOD: &str = "SUPER_MOD";
const PENDING: &str = "pending_";
const PATCHED: &str = "patched_";
const AMENDED: &str = "amended_";
const UNREGISTERED: &str = "UNREGISTERED_";

pub struct ProjectManager {
    main_dir: PathBuf,
}

impl ProjectManager {
    pub fn new(proj_dir: &str) -> Self {
        Self {
            main_dir: PathBuf::from(proj_dir),
        }
    }

    fn list_mods(&self, map_id: &str, prefix: &str) -> AnyHow<Vec<String>> {
        let path_file_name = |dir_entry: Result<std::fs::DirEntry, _>| -> AnyHow<String> {
            dir_entry?
                .file_name()
                .into_string()
                .map_err(|e| anyhow!("Non UTF-8 character found in file name: {:#?}", e))
        };

        let suffix = |name: String| -> AnyHow<String> {
            let suffix = name.split('_')
                .nth(1)
                .context("Invalid mod name")?;
            Ok(suffix.to_string())
        };

        self.main_dir
            .join(map_id)
            .join(MODS)
            .read_dir()?
            .map(path_file_name)
            .collect::<AnyHow<Vec<_>>>()?
            .into_iter()
            .filter(|name| name.starts_with(prefix))
            .map(suffix)
            .collect::<AnyHow<Vec<_>>>()
    }

    fn max_mod_id(&self, proj_id: &str) -> AnyHow<u32> {
        let suffixes = self.list_mods(proj_id, UNREGISTERED)?
            .into_iter()
            .map(|suffix| suffix.parse::<u32>().context("Invalid mod id"))
            .collect::<AnyHow<Vec<_>>>()?;

        Ok(suffixes.into_iter().max().unwrap_or(0))
    }

    pub fn new_project(&self, id: &str) -> AnyHow<()> {
        create_dir(self.main_dir.join(id)).map_err(|e| anyhow!("Project already exists: {}", e))
    }

    pub fn list_pending(&self, map_id: &str) -> AnyHow<()> {
        let pending = self.list_mods(map_id, PENDING)?;
        let patched = self.list_mods(map_id, PATCHED)?;

        let still_pending = pending
            .into_iter()
            .filter(|suffix| !patched.contains(suffix))
            .collect::<Vec<_>>();

        println!("{}", still_pending.join("\n"));
        Ok(())
    }

    pub fn gen_mod(&self, map_id: &str, original: &str, temp: &str, comment: &str) -> AnyHow<()> {
        let source = self
            .main_dir
            .join(map_id)
            .join(original)
            .read()?;
        
        let modded = self
            .main_dir
            .join(map_id)
            .join(temp)
            .read()?;

        let new_mod_name = format!("{}{}", UNREGISTERED, self.max_mod_id(map_id)? + 1);
        
        let modded_diff = {
            let struct_diff = StructDiff::build_from(
                source.as_str(),
                modded.as_str(),
                &comment.replace(IO_SEPARATOR, "[sanetized]")
            );
            
            let def = StructDiffDef::from(struct_diff);
            to_string_pretty(&def)?
        };
    
        self.main_dir
            .join(map_id)
            .join(MODS)
            .join(new_mod_name.as_str())
            .write_plus(&modded_diff)?;
        
        self.main_dir
            .join(map_id)
            .join(temp)
            .remove()
    }

    pub fn view_mod(&self, map_id: &str, original: &str, mod_id: &str, config: &str) -> AnyHow<()> {
        let source = self
            .main_dir
            .join(map_id)
            .join(original)
            .read()?;
        
        let mod_file = self
            .main_dir
            .join(map_id)
            .join(MODS)
            .join(mod_id)
            .read()?;

        let structure = Structure {
            contents: source.lines().map(|s| s.to_string()).collect(),
            config: from_str::<ConfigDef>(config)?.into()
        };

        let struct_diff: StructDiff = from_str::<StructDiffDef>(&mod_file)?.into();
        
        println!("{}", structure.forward_inflate(&struct_diff).contents.join("\n"));
        Ok(())
    }

    pub fn try_fold(&self, map_id: &str, original: &str, mod_id: &str, config: &str) -> AnyHow<()> {
        if !self.main_dir.join(MODS).join(SUPER_MOD).is_file() {
            let modded = self
                .main_dir
                .join(map_id)
                .join(MODS)
                .join(format!("{}{}", PENDING, mod_id).as_str());

            let super_mod = self
                .main_dir
                .join(map_id)
                .join(MODS)
                .join(SUPER_MOD);

            let patched = self
                .main_dir
                .join(map_id)
                .join(MODS)
                .join(format!("{}{}", PATCHED, mod_id).as_str());

            super_mod.copy_from(&modded)?;
            patched.copy_from(&modded)?;
            Ok(())
        }
        else {
            let mod_name = {
                let pending = format!("{}{}", PENDING, mod_id);
                let amended = format!("{}{}", AMENDED, mod_id);

                let hot = self
                    .main_dir
                    .join(MODS)
                    .join(&amended)
                    .is_file();

                if hot { amended } else { pending }
            };
       
            let original = self
                .main_dir
                .join(map_id)
                .join(original)
                .read()?;

            let super_mod = self
                .main_dir
                .join(map_id)
                .join(MODS)
                .join(SUPER_MOD)
                .read()?;

            let modded_path = self
                .main_dir
                .join(map_id)
                .join(MODS)
                .join(mod_name.as_str());

            let original = Structure {
                contents: original.lines().map(|s| s.to_string()).collect(),
                config: from_str::<ConfigDef>(config)?.into()
            };

            let mut super_mod: StructDiff = from_str::<StructDiffDef>(&super_mod)?.into();
            let modded: StructDiff = from_str::<StructDiffDef>(&modded_path.read()?)?.into();

            if let Some((conflicts_0, conflicts_1)) = original.conflicts(&super_mod, &modded) {
                println!("{}\n{}\n{}",
                    conflicts_0.contents.join("\n"),
                    IO_SEPARATOR,
                    conflicts_1.contents.join("\n")
                )
            }
            else {
                super_mod.extend(modded);
                let contents = to_string(&StructDiffDef::from(super_mod))?;

                self.main_dir
                    .join(map_id)
                    .join(MODS)
                    .join(SUPER_MOD)
                    .write_plus(&contents)?;

                self.main_dir
                    .join(map_id)
                    .join(MODS)
                    .join(format!("{}{}", PATCHED, mod_id).as_str())
                    .copy_from(&modded_path)?;
            }

            Ok(())
        }
    }

    pub fn amend_mod(&self, map_id: &str, original: &str, mod_id: &str, comment: &str) -> AnyHow<()> {
        let stdin = io::stdin();
        let mut stdin = stdin.lock();
        let new_contents = stdin.fill_buf()?;

        let original = self
            .main_dir
            .join(map_id)
            .join(original)
            .read()?;

        let modded = StructDiff::build_from(
            original.as_str(),
            std::str::from_utf8(new_contents)?,
            &comment.replace(IO_SEPARATOR, "[sanetized]")
        );

        self.main_dir
            .join(map_id)
            .join(MODS)
            .join(format!("{}{}", AMENDED, mod_id).as_str())
            .write_plus(&to_string(&StructDiffDef::from(modded))?)
    }

    pub fn skip_mod(&self, map_id: &str, mod_id: &str) -> AnyHow<()> {
        let mod_file = self
            .main_dir
            .join(map_id)
            .join(MODS)
            .join(format!("{}{}", PENDING, mod_id));
        
        if mod_file.is_file() {
            let patched_path = self
                .main_dir
                .join(map_id)
                .join(MODS)
                .join(format!("{}{}", PATCHED, mod_id));

            File::create(&patched_path)?
                .write_all(b"")
                .map_err(|e| anyhow!("Unable to create patch: {}", e))
        }
        else {
            Err(anyhow!("Pending mod not found"))
        }
    }

    pub fn map_check_sum(&self, map_id: &str, map_name: &str) -> AnyHow<u32> {
        let contents = self.main_dir
            .join(map_id)
            .join(map_name)
            .read()?;

        Ok(crc32fast::hash(contents.as_bytes()))
    }

    pub fn update_from(&self, map_id: &str, bytes: Vec<u8>) -> AnyHow<()> {
        let temp = self
            .main_dir
            .join(map_id)
            .join("temp");

        temp.write_plus("")?;
        
        File::create(&temp)?
            .write_all(&bytes)
            .map_err(|e| anyhow!("Unable to write to temp file: {}", e))?;

        zip::ZipArchive::new(File::open(&temp)?)?.extract(self.main_dir.join(map_id))?;

        temp.remove()
    }

    pub fn unregistered_mod_paths(&self, map_id: &str) -> AnyHow<Vec<PathBuf>> {
        let paths = self.list_mods(map_id, UNREGISTERED)?
            .into_iter()
            .map(|name| self.main_dir
                .join(map_id)
                .join(MODS)
                .join(format!("{}{}", UNREGISTERED, name))
            );

        Ok(paths.collect())
    }

    pub fn unsubmitted_patched(&self, map_id: &str) -> AnyHow<Vec<String>> {
        let pending = self.list_mods(map_id, PENDING)?;
        let patched = self.list_mods(map_id, PATCHED)?;
        
        Ok(
            pending
                .into_iter()
                .filter(|p| patched.contains(p))
                .collect()
        )
    }

    pub fn temp_patched(&self, map_id: &str, map_name: &str) -> AnyHow<PathBuf> {
        let source = self
            .main_dir
            .join(map_id)
            .join(map_name)
            .read()?;

        let super_mod = self.main_dir
            .join(map_id)
            .join(MODS)
            .join(SUPER_MOD)
            .read()?;

        let super_mod: StructDiff = from_str::<StructDiffDef>(super_mod.as_str())
            .context("Failed to deserialize super mod")?
            .into();

        let patched = super_mod.patch(source.lines()).join("\n");
        let temp_path = self
            .main_dir
            .join(map_id)
            .join("temp_patched");

        temp_path.write_plus(&patched)?;
        Ok(temp_path)
    }

    pub fn patch_map(&self, map_id: &str, map_name: &str) -> AnyHow<()> {
        let original = self.main_dir
            .join(map_id)
            .join(map_name);

        let temp = self.main_dir
            .join(map_id)
            .join("temp_patched");

        original.remove()?;
        rename(&temp, &original)
            .map_err(|e| anyhow!("Unable to patch map: {}", e))?;

        Ok(())
    }
}
