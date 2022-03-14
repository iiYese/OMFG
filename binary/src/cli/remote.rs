use std::{
    io::{Write, Read},
    fs::{File, rename}, path::{Path, PathBuf},
    env::current_exe
};

use reqwest::{
    StatusCode,
    blocking::{Client, multipart::Form}
};
use serde::{Serialize, Deserialize};
use anyhow::{Context, anyhow, bail, Result as AnyHow};
use mac_address::get_mac_address;
use short_crypt::ShortCrypt;

use crate::utils::{PathBufExt, DebugPrint};

use super::*;

#[derive(Serialize, Deserialize)]
struct Credentials {
    email: String,
    access_key: String,
}

pub struct ClientHandle {
    client: Client,
    server: String,
    credentials: Credentials,
}

impl ClientHandle {
    fn crypt() -> AnyHow<ShortCrypt> {
        let crypt = ShortCrypt::new(
            get_mac_address()?
                .ok_or(anyhow!("Could not get key for encryption"))?
                .to_string()
        );

        Ok(crypt)
    }

    pub fn save_credentials(email: &str, access_key: &str) -> AnyHow<()> {
        let crypt = Self::crypt()?;

        let credentials = Credentials {
            email: email.to_string(),
            access_key: access_key.to_string()
        };

        let (base, credentials) = crypt.encrypt(&serde_json::to_string(&credentials)?);
        
        let path = current_exe()?
            .parent()
            .context("Could not get parent directory")?
            .to_path_buf()
            .join("credentials");

        path.write_plus("")?;

        let mut file = File::create(path)
            .map_err(|e| anyhow!("Failed to open credentials file: {}", e))?;
        
        file.write_all(&[&[base], credentials.as_slice()].concat())
            .map_err(|e| anyhow!("Failed to write credentials: {}", e))
    }

    pub fn new(server: &str) -> AnyHow<Self> {
        let credential_path = current_exe()?
            .parent()
            .context("Could not get parent directory")?
            .to_path_buf()
            .join("credentials");
        
        let f = File::open(credential_path)?;
        let mut reader = std::io::BufReader::new(f);
        let mut contents = Vec::new();
        
        // Read file into vector.
        reader.read_to_end(&mut contents)?;

        let crypt = Self::crypt()?;

        if contents.len() < 2 {
            bail!("Corrupt credentials file");
        }

        let contents = crypt
            .decrypt(&(contents[0], contents[1..].to_vec()))
            .map_err(|e| anyhow!("Could not decrypt credentials: {}", e))?;
        
        Ok(
            ClientHandle {
                client: Client::new(),
                server: server.to_string(),
                credentials: serde_json::from_str(std::str::from_utf8(contents.as_slice())?)?
            }
        )
    }

    pub fn list_projects(&self) -> AnyHow<()>{
        let resp: ProjectList = self
            .client
            .post(&format!("{}/list_projects", self.server))
            .basic_auth(&self.credentials.email, Some(&self.credentials.access_key))
            .send()?
            .json()?;

        println!(
            "{:#?}",
            resp.extract()?
        );

        Ok(())
    }

    pub fn create_project(&self) -> AnyHow<String> {
        let resp: CreateProj = self
            .client
            .post(&format!("{}/create_project", self.server))
            .basic_auth(&self.credentials.email, Some(&self.credentials.access_key))
            .send()?
            .json()?;

        let id = resp.extract()?;
        println!("{}", id);
        Ok(id)
    }

    pub fn delete_project(&self, map_id: &str) -> AnyHow<()> {
        let resp: GenericResponse = self
            .client
            .post(&format!("{}/delete_project/{}", self.server, map_id))
            .basic_auth(&self.credentials.email, Some(&self.credentials.access_key))
            .send()?
            .json()?;

        resp.ok()
    }

    pub fn get_status(&self, map_id: &str) -> AnyHow<()> {
        let resp: ModdingStatus = self
            .client
            .post(&format!("{}/modding_status/{}", self.server, map_id))
            .send()?
            .json()?;

        println!("{}", resp.status);
        Ok(())
    }

    fn change_modding(&self, map_id: &str, to: &str) -> AnyHow<()> {
        let resp: GenericResponse = self
            .client
            .post(&format!("{}/{}_modding/{}", self.server, to, map_id))
            .basic_auth(&self.credentials.email, Some(&self.credentials.access_key))
            .send()?
            .json()?;

        resp.ok()
        // query server again to get new status
    }

    pub fn try_open(&self, map_id: &str) -> AnyHow<()> {
        self.change_modding(map_id, "open")
    }

    pub fn try_close(&self, map_id: &str) -> AnyHow<()> {
        self.change_modding(map_id, "close")
    }

    pub fn check_upto_date(&self, map_id: &str, sum: u32) -> AnyHow<()> {
        let resp: Checksum = self
            .client
            .get(&format!("{}/get_checksum/{}", self.server, map_id))
            .send()?
            .json()?;

        if resp.sum != sum {
            println!("Checksum mismatch");
        }

        println!("ok");
        Ok(())
    }

    pub fn submit_map(&self, map_id: &str, path: &Path) -> AnyHow<()> {
        let resp: GenericResponse = self
            .client
            .post(&format!("{}/update_map/{}", self.server, map_id))
            .basic_auth(&self.credentials.email, Some(&self.credentials.access_key))
            .multipart(Form::new().file("file", path)?)
            .send()?
            .json()?;

        resp.ok()
    }

    pub fn fetch_project(&self, map_id: &str) -> AnyHow<Vec<u8>> {
        let resp = self
            .client
            .post(&format!("{}/sync/{}", self.server, map_id))
            .basic_auth(&self.credentials.email, Some(&self.credentials.access_key))
            .send()?;

        match resp.status() {
            StatusCode::OK => {
                let bytes = resp.bytes()?;
                Ok(bytes.to_vec())
            },
            StatusCode::NOT_FOUND => {
                bail!("Project not found");
            },
            _ => {
                bail!("Failed to fetch project");
            }
        }
    }

    pub fn submit_mods(&self, map_id: &str, mod_paths: &[PathBuf]) -> AnyHow<()> {
        let zip_path = current_exe()?
            .parent()
            .context("Could not get parent directory")?
            .to_path_buf()
            .join("temp");

        let file = File::create(&zip_path)?;
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::FileOptions::default();

        for path in mod_paths {
            let name = path
                .file_name()
                .context("Could not get file name")?
                .to_str()
                .context("Could not convert to string")?;

            zip.start_file(name, options)?;
            zip.write_all(path.read()?.as_bytes())?;
        }

        zip.finish()?;

        let resp: ModSubmission = self
            .client
            .post(&format!("{}/submit_mods/{}", self.server, map_id))
            .basic_auth(&self.credentials.email, Some(&self.credentials.access_key))
            .multipart(Form::new().file("zip_file", &zip_path)?)
            .send()?
            .json()?;

        let new_ids = resp.ok()?;

        for path in mod_paths {
            let old = path
                .file_name()
                .context("Could not get file name")?
                .to_str()
                .context("Could not convert to string")?;

            let new = path.parent()
                .context("Could not get parent directory")?
                .to_path_buf()
                .join(&new_ids[old]);

            rename(path, &new)?;
        }

        Ok(())
    }

    pub fn submit_patches(&self, map_id: &str, temp_path: &Path, patched: Vec<String>) -> AnyHow<()> {
        let zip_path = current_exe()?
            .parent()
            .context("Could not get parent directory")?
            .to_path_buf()
            .join("temp");

        let file = File::create(&zip_path)?;
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::FileOptions::default();

        zip.start_file("map_file", options)?;
        zip.write_all(temp_path.to_path_buf().read()?.as_bytes())?;

        zip.start_file("changes.json", options)?;
        let changes_json = serde_json::to_string(&Patches{ patched })?;
        zip.write_all(changes_json.as_bytes())?;

        zip.finish()?;
        
        let resp: GenericResponse = self.client
            .post(&format!("{}/patch_mods/{}", self.server, map_id))
            .basic_auth(&self.credentials.email, Some(&self.credentials.access_key))
            .multipart(Form::new().file("zip_file", zip_path)?)
            .send()?
            .json()?;

        resp.ok()
    }
}
