We should integrate with other systems, explaining where data came from when we read it. When we receive data from Protein, there can be behind it many parts contributing data, File Sync, Organ Sync, and Blood Sync (integrations). The data that comes from somewhere, when changed, should affect that somewhere, bidirectional CRUD.


Export can produce a self-contained static view of supported Sands or a workspace. It freezes the chosen data, carries no access token and makes no network requests. The Bevy interface needs an exporter for the supported pieces; the old HTML exporter does not prove this is complete.

Published Sand packages can also travel between connected Organs without a central registry. Preserve their content hash, lineage, author, declared capabilities and licenses independently of where the bytes are stored. A recipient can inspect a package even when its renderer is unavailable. Distribution and permission to execute remain separate.
