use gveditor_core_api::{
    Manifest,
    ManifestInfo,
};
use tokio::fs;
use tokio::runtime::Runtime;

use gveditor_core_api::extensions::base::{
    Extension,
    ExtensionInfo,
};
use gveditor_core_api::extensions::client::ExtensionClient;
use gveditor_core_api::extensions::manager::{
    ExtensionsManager,
    LoadedExtension,
};
use gveditor_core_api::messaging::ExtensionMessages;

mod main_worker;

mod worker_extension;

use main_worker::create_main_worker;

use async_trait::async_trait;

use tokio_stream::wrappers::ReadDirStream;
use tokio_stream::StreamExt;

/// DenoExtension is a wrapper around Graviton extension api that makes use of deno_runtime to execute the extensions
#[allow(dead_code)]
struct DenoExtension {
    main_path: String,
    info: ManifestInfo,
    client: ExtensionClient,
    state_id: u8,
}

impl DenoExtension {
    pub fn new(main_path: &str, info: ManifestInfo, client: ExtensionClient, state_id: u8) -> Self {
        Self {
            main_path: main_path.to_string(),
            info,
            client,
            state_id,
        }
    }
}

impl Extension for DenoExtension {
    fn init(&mut self) {
        let main_path = self.main_path.clone();
        let client = self.client.clone();
        // TODO: Is there a better way rather than launching it in a different thread?
        std::thread::spawn(move || {
            let r = Runtime::new().unwrap();
            r.block_on(async move {
                create_main_worker(&main_path, client).await;
            });
        });
    }

    fn notify(&mut self, _message: ExtensionMessages) {}

    fn get_info(&self) -> ExtensionInfo {
        ExtensionInfo {
            name: self.info.extension.name.clone(),
            id: self.info.extension.id.clone(),
        }
    }
}

/// Add support for a special method that allows core invokers to execute Deno extensions
#[async_trait]
pub trait DenoExtensionSupport {
    fn load_extension_with_deno(
        &mut self,
        path: &str,
        info: ManifestInfo,
        state_id: u8,
    ) -> &mut ExtensionsManager;
    async fn load_extensions_with_deno_in_directory(
        &mut self,
        path: &str,
        state_id: u8,
    ) -> &mut ExtensionsManager;
}

#[async_trait]
impl DenoExtensionSupport for ExtensionsManager {
    fn load_extension_with_deno(
        &mut self,
        path: &str,
        info: ManifestInfo,
        state_id: u8,
    ) -> &mut ExtensionsManager {
        let client = ExtensionClient::new(&info.extension.name.clone(), self.sender.clone());
        let deno_extension = Box::new(DenoExtension::new(path, info.clone(), client, state_id));
        self.register(&info.extension.id, deno_extension);
        self.extensions
            .push(LoadedExtension::ManifestBuiltin { info });
        self
    }

    async fn load_extensions_with_deno_in_directory(
        &mut self,
        path: &str,
        state_id: u8,
    ) -> &mut ExtensionsManager {
        let items = fs::read_dir(path).await;

        if let Ok(items) = items {
            let mut items = ReadDirStream::new(items);

            // Iterate over all the found extensions
            while let Some(Ok(item)) = items.next().await {
                let item_path = item.path();
                let manifest_path = item_path.join("Graviton.toml");

                // Get the extension manifest
                let manifest = Manifest::parse(&manifest_path).await;

                if let Ok(manifest) = manifest {
                    // Load it's entry file if specified
                    if let Some(main) = &manifest.info.extension.main {
                        let client = ExtensionClient::new(
                            &manifest.info.extension.name,
                            self.sender.clone(),
                        );
                        let main_path = item_path.join(main);
                        let deno_extension = Box::new(DenoExtension::new(
                            main_path.to_str().unwrap(),
                            manifest.info.clone(),
                            client,
                            state_id,
                        ));
                        self.register(&manifest.info.extension.id, deno_extension);
                        self.extensions
                            .push(LoadedExtension::ManifestFile { manifest });
                    } else {
                        // Doesn't have an entry file
                    }
                }
            }
        }

        self
    }
}