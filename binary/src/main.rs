#![cfg_attr(test, allow(dead_code))]
mod cli;
mod struct_diff;
mod utils;

use std::{env, path::PathBuf};

use anyhow::{Context, anyhow};

use cli::*;
use utils::*;

fn main() {
    let collected = env::args().collect::<Vec<_>>();
    let args = &collected[1..]
        .iter()
        .map(|s| s.as_str())
        .collect::<Vec<_>>();

    match args.as_slice() {
        ["get-access-key", server_url] => {
            webbrowser::open(server_url)
                .map_err(|e| anyhow!("Could not open URL: {}", e))
                .unwrap();
        }
        ["save-credentials", email, access_key] => {
            ClientHandle::save_credentials(email, access_key)
                .map_err(|e| anyhow!("Could not save credentials: {}", e))
                .unwrap();
        }
        [main_projects_dir, operation @ ..] => {
            let project_manager = ProjectManager::new(main_projects_dir);
            match operation {
                ["list-pending", map_id] => {
                    project_manager
                        .list_pending(map_id)
                        .context("Failed to list pending")
                        .unwrap();
                }
                ["gen-mod", map_id, original, temp, comment] => {
                    project_manager
                        .gen_mod(map_id, original, temp, comment)
                        .context("Failed to generate mod")
                        .unwrap();
                }
                ["view-mod", map_id, original, mod_id, config] => {
                    project_manager
                        .view_mod(map_id, original, mod_id, config)
                        .context("Failed to inflate minimal from mod")
                        .unwrap();
                }
                ["try-fold", map_id, original, selected, config] => {
                    project_manager
                        .try_fold(map_id, original, selected, config)
                        .context("Failed to fold mods")
                        .unwrap();
                }
                ["amend-mod", map_id, original, selected, comment] => {
                    project_manager
                        .amend_mod(map_id, original, selected, comment)
                        .context("Failed to amend mod")
                        .unwrap();
                }
                ["skip-mod", map_id, selected] => {
                    project_manager
                        .skip_mod(map_id, selected)
                        .context("Failed to skip mod")
                        .unwrap();
                }
                ["reset-ammendments", _map_id, _selected] => {
                    unimplemented!()
                }
                [server_url, operation @ ..] => {
                    let client_handle = ClientHandle::new(server_url).unwrap();
                    match operation {
                        ["list-projects"] => {
                            client_handle
                                .list_projects()
                                .context("Failed to list projects")
                                .unwrap();
                        }
                        ["create-project"] => {
                            let id = client_handle
                                .create_project()
                                .context("Failed to create new project")
                                .unwrap();

                            project_manager
                                .new_project(&id)
                                .context("Failed generate new project files")
                                .unwrap();
                        }
                        ["delete-project", map_id] => {
                            client_handle
                                .delete_project(map_id)
                                .context("Failed to delete project")
                                .unwrap();
                        }
                        ["modding-status", map_id] => {
                            client_handle
                                .get_status(map_id)
                                .context("Failed to update modding status")
                                .unwrap();
                        }
                        ["try-open", map_id] => {
                            client_handle
                                .try_open(map_id)
                                .context("Failed to open mod")
                                .unwrap();
                        }
                        ["try-close", map_id] => {
                            client_handle
                                .try_close(map_id)
                                .context("Failed to close mod")
                                .unwrap();
                        }
                        ["check-version", map_id, map_name] => {
                            let sum = project_manager
                                .map_check_sum(map_id, map_name)
                                .context("Failed to get map checksum")
                                .unwrap();
                            
                            client_handle
                                .check_upto_date(map_id, sum)
                                .context("Failed to check version")
                                .unwrap();
                        }
                        ["submit-map", map_id, file_name] => {
                            let path = PathBuf::from(main_projects_dir)
                                .join(map_id)
                                .join(file_name);

                            client_handle
                                .submit_map(map_id, &path)
                                .context("Failed to submit map")
                                .unwrap();
                        }
                        ["sync", map_id] => {
                            let fetched = client_handle
                                .fetch_project(map_id)
                                .context("Failed to fetch project")
                                .unwrap();

                            project_manager
                                .update_from(map_id, fetched)
                                .context("Failed to update project")
                                .unwrap();
                        }
                        ["submit-mods", map_id] => {
                            let unregistered = project_manager
                                .unregistered_mod_paths(map_id)
                                .context("Failed to get unregistered mod paths")
                                .unwrap();

                            client_handle
                                .submit_mods(map_id, &unregistered)
                                .context("Failed to submit mods")
                                .unwrap();
                        },
                        ["submit-patches", map_id, map_name] => {
                            let temp_patched = project_manager
                                .temp_patched(map_id, map_name)
                                .context("Failed to get temp patched paths")
                                .unwrap();

                            let patched = project_manager
                                .unsubmitted_patched(map_id)
                                .context("Failed to get patched paths")
                                .unwrap()
                                .into_iter()
                                .map(|suffix| format!("pending_{}", suffix))
                                .collect::<Vec<_>>();

                            client_handle
                                .submit_patches(map_id, &temp_patched, patched)
                                .context("Failed to submit patches")
                                .unwrap();

                            project_manager
                                .patch_map(map_id, map_name)
                                .context("Failed to patch map")
                                .unwrap();
                        }
                        _ => panic!("Invalid arguments"),
                    }
                }
                _ => panic!("Invalid arguments"),
            }
        }
        _ => panic!("Invalid arguments"),
    };
}
