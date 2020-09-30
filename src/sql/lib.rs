// These functions are used to quickly perform drupal migrations by directly
// inserting and updating rows in the database rather than going through Drupal.
//
// It is meant only for testing purposes to save time (be warned)!
// It assumes that no content exists on the site when executed!!!
//
// In short this library generates an SQL script for importing only the basic
// properties for:
//
//  - files
//  - media
//  - media revisions
//  - nodes
//
// This takes the files generated by the `csv` command as input as well as the
// `users.csv` that needs to be generated manually (see the README.md).
//
// Also generates and populates the mapping tables, note that the last column
// used to track changes to the source is not populated by this script, so
// updates will overwrite the changes made by this script.
//
// e.g. Of migration mapping table: migrate_map_fedora_media
//+------------------------------------------------------------------+-----------+-----------+---------+-------------------+-----------------+---------------+------------------------------------------------------------------+
//| source_ids_hash                                                  | sourceid1 | sourceid2 | destid1 | source_row_status | rollback_action | last_imported | hash                                                             |
//+------------------------------------------------------------------+-----------+-----------+---------+-------------------+-----------------+---------------+------------------------------------------------------------------+
//| 000004fd2f49c175d5642673755c3ee43f90b5eebad2694ac52eda44496c611f | vcu:38191 | JPG       |  304977 |                 0 |               0 |             0 | a2f9248ceef1081dcff2deb8ebecbf680c6a956a790028de6ce1bbd175b8622d |
//+------------------------------------------------------------------+-----------+-----------+---------+-------------------+-----------------+---------------+------------------------------------------------------------------+

use crypto::digest::Digest;
use crypto::sha2::Sha256;
use csv::ReaderBuilder;
use indexmap::IndexMap; // Use instead of default HashMaps to preserver insertion order used to generate uid, fid, etc.
use serde::Deserialize;
use std::cell::RefCell;
use std::fs;
use std::io::Write;
use std::io::{BufReader, Seek, SeekFrom};
use std::path::Path;
use std::rc::Rc;
use std::time::SystemTime;
use tempfile::tempfile;
use uuid::Uuid;

// Migration mapping tables do not exist until a migration is run so we must
// create them here since this is intended to run before any content is created.
static CREATE_TABLES_PREAMBLE: &str = r#"
--
-- Table structure for table `migrate_map_fedora_users`
--

DROP TABLE IF EXISTS `migrate_map_fedora_users`;
/*!40101 SET @saved_cs_client     = @@character_set_client */;
/*!40101 SET character_set_client = utf8 */;
CREATE TABLE `migrate_map_fedora_users` (
  `source_ids_hash` varchar(64) NOT NULL COMMENT 'Hash of source ids. Used as primary key',
  `sourceid1` varchar(255) NOT NULL,
  `destid1` int(10) unsigned DEFAULT NULL,
  `source_row_status` tinyint(3) unsigned NOT NULL DEFAULT 0 COMMENT 'Indicates current status of the source row',
  `rollback_action` tinyint(3) unsigned NOT NULL DEFAULT 0 COMMENT 'Flag indicating what to do for this item on rollback',
  `last_imported` int(10) unsigned NOT NULL DEFAULT 0 COMMENT 'UNIX timestamp of the last time this row was imported',
  `hash` varchar(64) DEFAULT NULL COMMENT 'Hash of source row data, for detecting changes',
  PRIMARY KEY (`source_ids_hash`),
  KEY `source` (`sourceid1`(191))
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='Mappings from source identifier value(s) to destination…';
/*!40101 SET character_set_client = @saved_cs_client */;

--
-- Table structure for table `migrate_map_fedora_files`
--

DROP TABLE IF EXISTS `migrate_map_fedora_files`;
/*!40101 SET @saved_cs_client     = @@character_set_client */;
/*!40101 SET character_set_client = utf8 */;
CREATE TABLE `migrate_map_fedora_files` (
  `source_ids_hash` varchar(64) NOT NULL COMMENT 'Hash of source ids. Used as primary key',
  `sourceid1` varchar(255) NOT NULL,
  `sourceid2` varchar(255) NOT NULL,
  `sourceid3` varchar(255) NOT NULL,
  `destid1` int(10) unsigned DEFAULT NULL,
  `source_row_status` tinyint(3) unsigned NOT NULL DEFAULT 0 COMMENT 'Indicates current status of the source row',
  `rollback_action` tinyint(3) unsigned NOT NULL DEFAULT 0 COMMENT 'Flag indicating what to do for this item on rollback',
  `last_imported` int(10) unsigned NOT NULL DEFAULT 0 COMMENT 'UNIX timestamp of the last time this row was imported',
  `hash` varchar(64) DEFAULT NULL COMMENT 'Hash of source row data, for detecting changes',
  PRIMARY KEY (`source_ids_hash`),
  KEY `source` (`sourceid1`(191),`sourceid2`(191),`sourceid3`(191))
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='Mappings from source identifier value(s) to destination…';
/*!40101 SET character_set_client = @saved_cs_client */;

--
-- Table structure for table `migrate_map_fedora_media`
--

DROP TABLE IF EXISTS `migrate_map_fedora_media`;
/*!40101 SET @saved_cs_client     = @@character_set_client */;
/*!40101 SET character_set_client = utf8 */;
CREATE TABLE `migrate_map_fedora_media` (
  `source_ids_hash` varchar(64) NOT NULL COMMENT 'Hash of source ids. Used as primary key',
  `sourceid1` varchar(255) NOT NULL,
  `sourceid2` varchar(255) NOT NULL,
  `destid1` int(10) unsigned DEFAULT NULL,
  `source_row_status` tinyint(3) unsigned NOT NULL DEFAULT 0 COMMENT 'Indicates current status of the source row',
  `rollback_action` tinyint(3) unsigned NOT NULL DEFAULT 0 COMMENT 'Flag indicating what to do for this item on rollback',
  `last_imported` int(10) unsigned NOT NULL DEFAULT 0 COMMENT 'UNIX timestamp of the last time this row was imported',
  `hash` varchar(64) DEFAULT NULL COMMENT 'Hash of source row data, for detecting changes',
  PRIMARY KEY (`source_ids_hash`),
  KEY `source` (`sourceid1`(191),`sourceid2`(191))
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='Mappings from source identifier value(s) to destination…';
/*!40101 SET character_set_client = @saved_cs_client */;

--
-- Table structure for table `migrate_map_fedora_media_revisions`
--

DROP TABLE IF EXISTS `migrate_map_fedora_media_revisions`;
/*!40101 SET @saved_cs_client     = @@character_set_client */;
/*!40101 SET character_set_client = utf8 */;
CREATE TABLE `migrate_map_fedora_media_revisions` (
  `source_ids_hash` varchar(64) NOT NULL COMMENT 'Hash of source ids. Used as primary key',
  `sourceid1` varchar(255) NOT NULL,
  `sourceid2` varchar(255) NOT NULL,
  `sourceid3` varchar(255) NOT NULL,
  `destid1` int(10) unsigned DEFAULT NULL,
  `source_row_status` tinyint(3) unsigned NOT NULL DEFAULT 0 COMMENT 'Indicates current status of the source row',
  `rollback_action` tinyint(3) unsigned NOT NULL DEFAULT 0 COMMENT 'Flag indicating what to do for this item on rollback',
  `last_imported` int(10) unsigned NOT NULL DEFAULT 0 COMMENT 'UNIX timestamp of the last time this row was imported',
  `hash` varchar(64) DEFAULT NULL COMMENT 'Hash of source row data, for detecting changes',
  PRIMARY KEY (`source_ids_hash`),
  KEY `source` (`sourceid1`(191),`sourceid2`(191),`sourceid3`(191))
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='Mappings from source identifier value(s) to destination…';
/*!40101 SET character_set_client = @saved_cs_client */;

--
-- Table structure for table `migrate_map_fedora_nodes`
--

DROP TABLE IF EXISTS `migrate_map_fedora_nodes`;
/*!40101 SET @saved_cs_client     = @@character_set_client */;
/*!40101 SET character_set_client = utf8 */;
CREATE TABLE `migrate_map_fedora_nodes` (
  `source_ids_hash` varchar(64) NOT NULL COMMENT 'Hash of source ids. Used as primary key',
  `sourceid1` varchar(255) NOT NULL,
  `destid1` int(10) unsigned DEFAULT NULL,
  `source_row_status` tinyint(3) unsigned NOT NULL DEFAULT 0 COMMENT 'Indicates current status of the source row',
  `rollback_action` tinyint(3) unsigned NOT NULL DEFAULT 0 COMMENT 'Flag indicating what to do for this item on rollback',
  `last_imported` int(10) unsigned NOT NULL DEFAULT 0 COMMENT 'UNIX timestamp of the last time this row was imported',
  `hash` varchar(64) DEFAULT NULL COMMENT 'Hash of source row data, for detecting changes',
  PRIMARY KEY (`source_ids_hash`),
  KEY `source` (`sourceid1`(191))
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='Mappings from source identifier value(s) to destination…';
/*!40101 SET character_set_client = @saved_cs_client */;
"#;

// Like PHP serialize(), but limited to a list of strings as input.
// i.e. serialize(array("pid")); => a:2:{i:0;s:3:"pid";}
// Used to generate source ids for migrate map tables.
fn serialize(values: &[&str]) -> String {
    let mut result = String::new();
    result.push_str("a:");
    result += &values.len().to_string();
    result.push_str(":{");
    values.iter().enumerate().for_each(|(i, v)| {
        result.push_str("i:");
        result += &i.to_string();
        result.push_str(";");
        result.push_str("s:");
        result += &v.len().to_string();
        result.push_str(r#":""#);
        result += *v;
        result.push_str(r#"";"#);
    });
    result.push_str("}");
    result
}

fn hash(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.input_str(value);
    hasher.result_str()
}

fn source_ids_hash(values: &[&str]) -> String {
    hash(&serialize(&values))
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[derive(Debug)]
enum Error {
    CSVError(csv::Error),
    IOError(std::io::Error),
}

impl From<csv::Error> for Error {
    fn from(error: csv::Error) -> Self {
        Error::CSVError(error)
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Error::IOError(error)
    }
}

type Result<T> = std::result::Result<T, Error>;

struct Table {
    name: &'static str,
    columns: Vec<&'static str>,
    values: Vec<String>,
}

impl Table {
    fn dump(&self, file: &mut fs::File) -> Result<()> {
        file.write_all(
            format!(
                r#"
--
-- Dumping data for table `{table}`
--

LOCK TABLES `{table}` WRITE;
/*!40000 ALTER TABLE `{table}` DISABLE KEYS */;
set autocommit=0;
INSERT INTO `{table}` ({columns}) VALUES ({values});
/*!40000 ALTER TABLE `{table}` ENABLE KEYS */;
UNLOCK TABLES;
commit;
"#,
                table = self.name,
                columns = self.columns.join(","),
                values = self.values.join(",")
            )
            .as_bytes(),
        )?;
        Ok(())
    }
}

trait TableSerializer {
    fn tables(&self) -> Vec<Table>;

    fn dump(&self, mut file: &mut fs::File) -> Result<()> {
        self.tables()
            .iter()
            .map(|table| table.dump(&mut file))
            .collect()
    }
}

trait SourceRow: Sized + serde::de::DeserializeOwned {
    fn id() -> IdMaps;

    fn offset() -> usize {
        0
    }

    fn csv(path: &Path) -> Result<fs::File>;

    fn source_ids(&self) -> Vec<&str>;

    fn source_ids_hash(&self) -> String {
        hash(&serialize(&self.source_ids()))
    }
}

trait SourceRows: Sized {
    type Row: SourceRow;
    fn new(path: &Path, ids: SharedTableIdMaps) -> Result<Self>;
    fn map(csv: &fs::File) -> Result<IndexMap<String, Self::Row>>;
    fn ids(&self) -> TableIdMap;
    fn uid(&self, user: &str) -> usize;
    fn mid(&self, pid: &str, dsid: &str) -> usize;
}

#[derive(PartialEq, Eq, Hash, Debug)]
enum IdMaps {
    FID, // File ID
    MID, // Media ID
    NID, // Node ID
    UID, // User ID
    VID, // Media Revision ID
}

type TableIdMap = IndexMap<String, usize>; // Map hashes or values to table indices to fetch uid, mid, etc.
type TableIdMaps = IndexMap<IdMaps, TableIdMap>; // Named, table id maps, allow the migration map to look up uid, mid, etc.
type SharedTableIdMaps = Rc<RefCell<TableIdMaps>>;

struct MigrateMap<T>
where
    T: SourceRow,
{
    map: IndexMap<String, T>, // Map source id hash to source row.
    ids: SharedTableIdMaps,   // Look up uid, mid, etc.
}

impl<T> MigrateMap<T>
where
    T: SourceRow,
{
    // Take the offset into consideration.
    fn rows(&self) -> impl std::iter::Iterator<Item = (usize, (&String, &T))> + '_ {
        self.map
            .iter()
            .enumerate()
            .map(|(index, row)| (T::offset() + index, row))
    }

    fn values<F>(&self, map: F) -> Vec<String>
    where
        F: Fn((usize, (&String, &T))) -> String,
    {
        self.rows().map(map).collect()
    }

    fn migrate_map_values(&self) -> Vec<String> {
        self.values(|(index, (hash, row))| {
            let source_ids = row.source_ids().join(",");
            format!("({},{},{})", hash, source_ids, index)
        })
    }
}

impl<T> SourceRows for MigrateMap<T>
where
    T: SourceRow,
{
    type Row = T;
    fn new(path: &Path, ids: SharedTableIdMaps) -> Result<Self> {
        Ok(Self {
            map: Self::map(&T::csv(&path)?)?,
            ids,
        })
    }

    fn map(csv: &fs::File) -> Result<IndexMap<String, T>> {
        let map = ReaderBuilder::new()
            .has_headers(true)
            .from_reader(BufReader::new(csv))
            .into_deserialize()
            .collect::<std::result::Result<Vec<T>, csv::Error>>()
            .map_err(Error::from)?
            .into_iter()
            .map(|row| (row.source_ids_hash(), row))
            .collect();
        Ok(map)
    }

    fn ids(&self) -> TableIdMap {
        self.map
            .iter()
            .enumerate()
            .map(|(index, (hash, _))| (hash.clone(), Self::Row::offset() + index))
            .collect()
    }

    fn uid(&self, user: &str) -> usize {
        // Special case since we do not migrate the admin user as the system creates it.
        if user == "admin" {
            1
        } else {
            let hash = source_ids_hash(&[user]);
            let index = UserRow::id();
            self.ids.borrow()[&index][hash.as_str()]
        }
    }

    fn mid(&self, pid: &str, dsid: &str) -> usize {
        let hash = source_ids_hash(&[pid, dsid]);
        let index = MediaRow::id();
        self.ids.borrow()[&index][hash.as_str()]
    }
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct UserRow {
    name: String,
    pass: String,
    mail: String,
    status: String,
    timezone: String,
    language: String,
}

impl SourceRow for UserRow {
    fn id() -> IdMaps {
        IdMaps::UID
    }

    fn offset() -> usize {
        2
    }

    fn csv(path: &Path) -> Result<fs::File> {
        Ok(fs::File::open(path.join("users.csv"))?)
    }

    fn source_ids(&self) -> Vec<&str> {
        vec![self.name.as_str()]
    }
}

type MigrateUserMap = MigrateMap<UserRow>;

impl TableSerializer for MigrateUserMap {
    fn tables(&self) -> Vec<Table> {
        vec![
            Table {
                name: "users",
                columns: vec!["uid", "uuid", "langcode"],
                values: self.values(|(index, _)| {
                    let uuid = Uuid::new_v4();
                    format!("({},{},'en')", index, uuid)
                }),
            },
            Table {
                name: "users_field_data",
                columns: vec![
                    "uid",
                    "langcode",
                    "name",
                    "created",
                    "access",
                    "default_langcode",
                ],
                values: self.values(|(index, (_, user))| {
                    format!("({},'en',{},{},0,1)", index, user.name, now())
                }),
            },
            Table {
                name: "migrate_map_fedora_users",
                columns: vec!["source_ids_hash", "sourceid1", "destid1"],
                values: self.migrate_map_values(),
            },
        ]
    }
}

#[derive(Deserialize)]
struct FileRow {
    pid: String,
    dsid: String,
    version: String,
    created_date: String,
    mime_type: String,
    name: String,
    path: String,
    user: String,
    sha1: String,
    size: String,
}

impl SourceRow for FileRow {
    fn id() -> IdMaps {
        IdMaps::FID
    }

    fn csv(path: &Path) -> Result<fs::File> {
        Ok(fs::File::open(path.join("files.csv"))?)
    }

    fn source_ids(&self) -> Vec<&str> {
        vec![self.pid.as_str(), self.dsid.as_str(), self.version.as_str()]
    }
}

type MigrateFileMap = MigrateMap<FileRow>;

impl TableSerializer for MigrateFileMap {
    fn tables(&self) -> Vec<Table> {
        vec![
            Table {
                name: "file_managed",
                columns: vec![
                    "fid", "uuid", "langcode", "uid", "filename", "uri", "filemime", "filesize",
                    "status", "created", "changed",
                ],
                values: self.values(|(index, (_, file))| {
                    format!(
                        "({},{},'en',{},{},{},{},{},{},{})",
                        index,
                        Uuid::new_v4(),
                        self.uid(&file.user),
                        &file.name,
                        &file.path,
                        &file.mime_type,
                        &file.size,
                        &file.created_date,
                        now()
                    )
                }),
            },
            Table {
                name: "filehash",
                columns: vec!["fid", "sha1"],
                values: self.values(|(index, (_, file))| format!("({},{})", index, &file.sha1)),
            },
            Table {
                name: "migrate_map_fedora_files",
                columns: vec![
                    "source_ids_hash",
                    "sourceid1",
                    "sourceid2",
                    "sourceid3",
                    "destid1",
                ],
                values: self.migrate_map_values(),
            },
        ]
    }
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct MediaRow {
    pid: String,
    dsid: String,
    version: String,
    bundle: String,
    created_date: String,
    file_size: String,
    label: String,
    mime_type: String,
    name: String,
    user: String,
}

impl SourceRow for MediaRow {
    fn id() -> IdMaps {
        IdMaps::MID
    }

    fn csv(path: &Path) -> Result<fs::File> {
        Ok(fs::File::open(path.join("media.csv"))?)
    }

    fn source_ids(&self) -> Vec<&str> {
        vec![self.pid.as_str(), self.dsid.as_str()]
    }
}

type MigrateMediaMap = MigrateMap<MediaRow>;

impl TableSerializer for MigrateMediaMap {
    fn tables(&self) -> Vec<Table> {
        vec![
            Table {
                name: "media",
                columns: vec!["mid", "vid", "bundle", "uuid", "langcode"],
                values: self.values(|(index, (_, media))| {
                    format!(
                        "({},{},{},{},'en')",
                        index,
                        index,
                        &media.bundle,
                        Uuid::new_v4()
                    )
                }),
            },
            Table {
                name: "media_field_data",
                columns: vec![
                    "mid",
                    "vid",
                    "bundle",
                    "langcode",
                    "status",
                    "name",
                    "created",
                    "changed",
                    "default_langcode",
                ],
                values: self.values(|(index, (_, media))| {
                    format!(
                        "({},{},{},'en',1,{},{},{},{}, 1)",
                        index,
                        index,
                        &media.bundle,
                        self.uid(&media.user),
                        &media.name,
                        &media.created_date,
                        &media.created_date,
                    )
                }),
            },
            Table {
                name: "migrate_map_fedora_media",
                columns: vec!["source_ids_hash", "sourceid1", "sourceid2", "destid1"],
                values: self.migrate_map_values(),
            },
        ]
    }
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct MediaRevisionRow {
    pid: String,
    dsid: String,
    version: String,
    bundle: String,
    created_date: String,
    file_size: String,
    label: String,
    mime_type: String,
    name: String,
    user: String,
}

impl SourceRow for MediaRevisionRow {
    fn id() -> IdMaps {
        IdMaps::VID
    }

    fn csv(path: &Path) -> Result<fs::File> {
        // Media rows are also part of media_revisions so we merge the two files
        // with the media.csv being first to preserve the correct order for mid
        // and vid. Additionally we need to remove the additional header in
        // media_revisions.csv.
        let mut csv = tempfile()?;
        csv.write_all(&fs::read(path.join("media.csv"))?)?;
        let media_revisions = fs::read_to_string(path.join("media_revisions.csv"))?
            .lines()
            .skip(1)
            .collect::<Vec<&str>>()
            .join("\n");
        csv.write_all(&media_revisions.as_bytes())?;
        csv.seek(SeekFrom::Start(0)).unwrap();
        Ok(csv)
    }

    fn source_ids(&self) -> Vec<&str> {
        vec![self.pid.as_str(), self.dsid.as_str(), self.version.as_str()]
    }
}

type MigrateMediaRevisionMap = MigrateMap<MediaRevisionRow>;

impl TableSerializer for MigrateMediaRevisionMap {
    fn tables(&self) -> Vec<Table> {
        vec![
            Table {
                name: "media_revision",
                columns: vec![
                    "mid",
                    "vid",
                    "langcode",
                    "revision_user",
                    "revision_created",
                    "revision_default",
                ],
                values: self.values(|(index, (_, media_revision))| {
                    format!(
                        "({},{},'en',{},{},1)",
                        index,
                        index,
                        self.uid(&media_revision.user),
                        &media_revision.created_date
                    )
                }),
            },
            Table {
                name: "media_field_revision",
                columns: vec![
                    "mid",
                    "vid",
                    "langcode",
                    "status",
                    "name",
                    "created",
                    "changed",
                    "default_langcode",
                ],
                values: self.values(|(index, (_, media))| {
                    format!(
                        "({},{},'en',1,{},{},{},{}, 1)",
                        self.mid(&media.pid, &media.dsid),
                        index,
                        self.uid(&media.user),
                        &media.name,
                        &media.created_date,
                        &media.created_date,
                    )
                }),
            },
            Table {
                name: "migrate_map_fedora_media_revisions",
                columns: vec![
                    "source_ids_hash",
                    "sourceid1",
                    "sourceid2",
                    "sourceid3",
                    "destid1",
                ],
                values: self.migrate_map_values(),
            },
        ]
    }
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct NodeRow {
    pid: String,
    created_date: String,
    label: String,
    weight: String,
    model: String,
    modified_date: String,
    state: String,
    user: String,
    display_hint: String,
    parents: String,
}

impl SourceRow for NodeRow {
    fn id() -> IdMaps {
        IdMaps::NID
    }

    fn csv(path: &Path) -> Result<fs::File> {
        Ok(fs::File::open(path.join("nodes.csv"))?)
    }

    fn source_ids(&self) -> Vec<&str> {
        vec![self.pid.as_str()]
    }
}

type MigrateNodeMap = MigrateMap<NodeRow>;

impl TableSerializer for MigrateNodeMap {
    fn tables(&self) -> Vec<Table> {
        vec![
            Table {
                name: "node",
                columns: vec!["nid", "vid", "type", "uuid", "langcode"],
                values: self.values(|(index, _)| {
                    format!(
                        "({},{},'islandora_object',{},'en')",
                        index,
                        index,
                        Uuid::new_v4()
                    )
                }),
            },
            Table {
                name: "node_field_data",
                columns: vec![
                    "nid",
                    "vid",
                    "type",
                    "langcode",
                    "status",
                    "uid",
                    "title",
                    "created",
                    "changed",
                    "promote",
                    "sticky",
                    "default_langcode",
                ],
                values: self.values(|(index, (_, node))| {
                    format!(
                        "({},{},'islandora_object','en',1,{},{},{},{},1,0,1)",
                        index,
                        index,
                        self.uid(&node.user),
                        &node.label,
                        &node.created_date,
                        &node.modified_date,
                    )
                }),
            },
            Table {
                name: "node_field_revision",
                columns: vec![
                    "nid",
                    "vid",
                    "langcode",
                    "status",
                    "uid",
                    "title",
                    "created",
                    "changed",
                    "promote",
                    "sticky",
                    "default_langcode",
                ],
                values: self.values(|(index, (_, node))| {
                    format!(
                        "({},{},'en',1,{},{},{},{},1,0,1)",
                        index,
                        index,
                        self.uid(&node.user),
                        &node.label,
                        &node.created_date,
                        &node.modified_date,
                    )
                }),
            },
            Table {
                name: "migrate_map_fedora_nodes",
                columns: vec!["source_ids_hash", "sourceid1", "destid1"],
                values: self.migrate_map_values(),
            },
        ]
    }
}

pub fn valid_source_directory(path: &Path) -> std::result::Result<(), String> {
    fn valid_directory(path: &Path) -> std::result::Result<(), String> {
        if path.is_dir() {
            Ok(())
        } else {
            Err(format!("The directory '{}' does not exist", path.display()))
        }
    }
    valid_directory(&path)?;
    vec![
        "files.csv",
        "media.csv",
        "media_revisions.csv",
        "nodes.csv",
        "users.csv",
    ]
    .into_iter()
    .map(|file| {
        let path = path.join(file);
        if path.is_file() && path.exists() {
            Ok(())
        } else {
            Err(format!("The file '{}' does not exist", path.display()))
        }
    })
    .collect::<std::result::Result<Vec<_>, String>>()?;
    Ok(())
}

fn dump<T>(mut file: &mut fs::File, path: &Path, ids: SharedTableIdMaps) -> Result<()>
where
    T: SourceRows + TableSerializer,
{
    let table_id_map = {
        let map = T::new(&path, ids.clone())?;
        map.dump(&mut file)?;
        map.ids()
    };
    ids.borrow_mut().insert(T::Row::id(), table_id_map);
    Ok(())
}

fn write_tables(path: &Path, mut file: fs::File) -> Result<()> {
    let ids = SharedTableIdMaps::new(RefCell::new(TableIdMaps::new()));
    dump::<MigrateUserMap>(&mut file, &path, ids.clone())?;
    dump::<MigrateFileMap>(&mut file, &path, ids.clone())?;
    dump::<MigrateMediaMap>(&mut file, &path, ids.clone())?;
    dump::<MigrateMediaRevisionMap>(&mut file, &path, ids.clone())?;
    dump::<MigrateNodeMap>(&mut file, &path, ids)?;
    Ok(())
}

pub fn generate_sql(input: &Path, dest: &Path) {
    let mut file = fs::File::create(dest.join("migrate.sql")).unwrap();
    file.write_all(&CREATE_TABLES_PREAMBLE.as_bytes()).unwrap();
    write_tables(&input, file).unwrap();
}

#[cfg(test)]
mod tests {

    #[test]
    fn serialize() {
        let values = vec!["namespace:123"];
        let expected = r#"a:1:{i:0;s:13:"namespace:123";}"#.to_string();
        let result = super::serialize(&values);
        assert_eq!(result, expected);
        let values = vec!["namespace:123", "DC"];
        let expected = r#"a:2:{i:0;s:13:"namespace:123";i:1;s:2:"DC";}"#.to_string();
        let result = super::serialize(&values);
        assert_eq!(result, expected);
    }

    #[test]
    fn source_hash() {
        let values = vec!["vcu:38191", "JPG"];
        let result = super::serialize(&values);
        let expected = r#"a:2:{i:0;s:9:"vcu:38191";i:1;s:3:"JPG";}"#.to_string();
        assert_eq!(result, expected);
        let expected = "000004fd2f49c175d5642673755c3ee43f90b5eebad2694ac52eda44496c611f";
        let result = super::hash(&result);
        assert_eq!(result, expected);
    }
}
