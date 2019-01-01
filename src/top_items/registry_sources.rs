// Copyright 2017-2019 Matthias Krüger. See the COPYRIGHT
// file at the top-level directory of this distribution.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::cmp::Ordering;
use std::fs;
use std::path::PathBuf;

use crate::cache::dircache::DirCache;
use crate::top_items::common::{dir_exists, format_table};

use humansize::{file_size_opts, FileSize};
use rayon::iter::*;
use walkdir::WalkDir;

#[derive(Clone, Debug)]
struct FileDesc {
    path: PathBuf,
    name: String,
    size: u64,
}

impl FileDesc {
    pub(crate) fn new_from_reg_src(path: &PathBuf) -> Self {
        let last_item = path.file_name().unwrap().to_str().unwrap().to_string();
        let mut i = last_item.split('-').collect::<Vec<_>>();
        i.pop();
        let name = i.join("-");
        let walkdir = WalkDir::new(path.display().to_string());

        let size = walkdir
            .into_iter()
            .map(|e| e.unwrap().path().to_owned())
            .filter(|f| f.exists())
            .collect::<Vec<_>>()
            .par_iter()
            .map(|f| {
                fs::metadata(f)
                    .unwrap_or_else(|_| {
                        panic!("Failed to get metadata of file '{}'", &path.display())
                    })
                    .len()
            })
            .sum();

        Self {
            name,
            size,
            path: path.into(),
        }
    } // fn new_from_reg_src()
}

#[derive(Clone, Debug, Eq)]
pub(crate) struct RgSrcInfo {
    name: String,
    size: u64,
    counter: u32,
    total_size: u64, // sort by this
}

impl RgSrcInfo {
    fn new(path: &PathBuf, counter: u32, total_size: u64) -> Self {
        let name: String;
        let size: u64;
        if path.exists() {
            size = fs::metadata(&path)
                .unwrap_or_else(|_| panic!("Failed to get metadata of file '{}'", &path.display()))
                .len();
            let n = path.file_name().unwrap().to_str().unwrap().to_string();
            let mut v = n.split('-').collect::<Vec<_>>();
            v.pop();
            name = v.join("-");
        } else {
            name = path
                .file_name()
                .unwrap()
                .to_os_string()
                .into_string()
                .unwrap();
            size = 0;
        }
        Self {
            name,
            size,
            counter,
            total_size,
        }
    }
}

impl PartialOrd for RgSrcInfo {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for RgSrcInfo {
    fn cmp(&self, other: &Self) -> Ordering {
        self.total_size.cmp(&other.total_size)
    }
}
impl PartialEq for RgSrcInfo {
    fn eq(&self, other: &Self) -> bool {
        self.total_size == other.total_size
    }
}

// registry sources (tarballs)
fn file_desc_list_from_path(cache: &mut DirCache) -> Vec<FileDesc> {
    cache
        .registry_sources
        .checkout_folders()
        .iter()
        .map(|path| FileDesc::new_from_reg_src(path))
        .collect::<Vec<_>>()
}

fn stats_from_file_desc_list(file_descs: Vec<FileDesc>) -> Vec<RgSrcInfo> {
    struct Pair {
        current: Option<FileDesc>,
        previous: Option<FileDesc>,
    }
    // take our list of file information and calculate the actual stats

    let mut out: Vec<RgSrcInfo> = Vec::new();
    let mut regcacheinfo: RgSrcInfo = RgSrcInfo::new(&PathBuf::from("ERROR 1/err1"), 0, 0);
    let mut counter: u32 = 0; // how many of a crate do we have
    let mut total_size: u64 = 0; // total size of these crates
                                 // iterate over the files
    let mut iter = file_descs.into_iter();

    let mut state = Pair {
        current: None,
        previous: None,
    };

    // start looping
    state.previous = state.current;
    state.current = iter.next();

    // loop until .previous and .current are None which means we are at the end
    while state.previous.is_some() || state.current.is_some() {
        match &state {
            Pair {
                current: None,
                previous: None,
            } => {
                // we reached the end of the queue
            }

            Pair {
                current: Some(current),
                previous: None,
            } => {
                // this should always be first line ever
                //@TODO assert that its empty
                // compute line but don't save it
                let current_size = &current.size;
                total_size += current_size;
                counter += 1;

                regcacheinfo = RgSrcInfo::new(&current.path, counter, total_size);
            }

            Pair {
                current: Some(current),
                previous: Some(previous),
            } => {
                if current.name == previous.name {
                    // update line but don't save it
                    // @todo assert that regcacheinfo is not empty
                    let current_size = &current.size;
                    total_size += current_size;
                    counter += 1;

                    regcacheinfo = RgSrcInfo::new(&current.path, counter, total_size);
                } else if current.name != previous.name {
                    // save old line
                    //  todo assert that regcacheinfo is not empty
                    out.push(regcacheinfo);
                    // reset counters
                    counter = 0;
                    total_size = 0;
                    // and update line
                    let current_size = &current.size;
                    total_size += current_size;
                    counter += 1;

                    regcacheinfo = RgSrcInfo::new(&current.path, counter, total_size);
                }
            }

            Pair {
                current: None,
                previous: Some(_previous),
            } => {
                // save old line
                // todo assert that regcacheinfo is not empty
                out.push(regcacheinfo);
                regcacheinfo = RgSrcInfo::new(&PathBuf::from("ERROR 2/err2"), 0, 0);

                // reset counters
                counter = 0;
                total_size = 0;
            }
        };

        // switch and queue next()
        state.previous = state.current;
        state.current = iter.next();
    }
    out
}
pub(crate) fn reg_src_list_to_string(limit: u32, mut collections_vec: Vec<RgSrcInfo>) -> String {
    if collections_vec.is_empty() {
        return String::new();
    }

    // sort the RepoImfo Vec in reverse, biggest item first
    collections_vec.sort();
    collections_vec.reverse();

    let mut table_matrix: Vec<Vec<String>> = Vec::new();

    table_matrix.push(vec![
        String::from("Name"),
        String::from("Count"),
        String::from("Average"),
        String::from("Total"),
    ]);

    for regsrc in collections_vec.into_iter().take(limit as usize) {
        let average_size = (regsrc.total_size / u64::from(regsrc.counter))
            .file_size(file_size_opts::DECIMAL)
            .unwrap();

        let total_size = regsrc
            .total_size
            .file_size(file_size_opts::DECIMAL)
            .unwrap();

        table_matrix.push(vec![
            regsrc.name,
            regsrc.counter.to_string(),
            average_size,
            total_size,
        ]);
    }
    format_table(&table_matrix)
}

pub(crate) fn registry_source_stats(
    path: &PathBuf,
    limit: u32,
    mut cache: &mut DirCache,
) -> String {
    let mut stdout = String::new();
    // don't crash if the directory does not exist (issue #9)
    if !dir_exists(path) {
        return stdout;
    }

    stdout.push_str(&format!(
        "\nSummary of: {} ({} total)\n",
        path.display(),
        cache
            .registry_sources
            .total_size()
            .file_size(file_size_opts::DECIMAL)
            .unwrap()
    ));

    let file_descs: Vec<FileDesc> = file_desc_list_from_path(&mut cache);
    let summary: Vec<RgSrcInfo> = stats_from_file_desc_list(file_descs);
    let string = reg_src_list_to_string(limit, summary);
    stdout.push_str(&string);

    stdout
}

#[cfg(test)]
mod top_crates_registry_sources {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn stats_from_file_desc_none() {
        // empty list
        let list: Vec<FileDesc> = Vec::new();
        let stats = stats_from_file_desc_list(list);
        let is = reg_src_list_to_string(4, stats);
        let empty = String::new();
        assert_eq!(is, empty);
    }

    #[test]
    fn stats_from_file_desc_one() {
        let fd = FileDesc {
            path: PathBuf::from("crateA"),
            name: "crateA".to_string(),
            size: 1,
        };
        let list_fd: Vec<FileDesc> = vec![fd];
        let list_cb: Vec<RgSrcInfo> = stats_from_file_desc_list(list_fd);
        let is: String = reg_src_list_to_string(1, list_cb);
        let wanted = String::from("Name   Count Average Total\ncrateA 1     1 B     1 B\n");
        assert_eq!(is, wanted);
    }

    #[test]
    fn stats_from_file_desc_two() {
        let fd1 = FileDesc {
            path: PathBuf::from("crate-A"),
            name: "crate-A".to_string(),
            size: 1,
        };
        let fd2 = FileDesc {
            path: PathBuf::from("crate-B"),
            name: "crate-B".to_string(),
            size: 2,
        };
        let list_fd: Vec<FileDesc> = vec![fd1, fd2];
        let list_cb: Vec<RgSrcInfo> = stats_from_file_desc_list(list_fd);
        let is: String = reg_src_list_to_string(3, list_cb);

        let mut wanted = String::new();
        for i in &[
            "Name    Count Average Total\n",
            "crate-B 1     2 B     2 B\n",
            "crate-A 1     1 B     1 B\n",
        ] {
            wanted.push_str(i);
        }
        assert_eq!(is, wanted);
    }

    #[test]
    fn stats_from_file_desc_multiple() {
        let fd1 = FileDesc {
            path: PathBuf::from("crate-A"),
            name: "crate-A".to_string(),
            size: 1,
        };
        let fd2 = FileDesc {
            path: PathBuf::from("crate-B"),
            name: "crate-B".to_string(),
            size: 2,
        };
        let fd3 = FileDesc {
            path: PathBuf::from("crate-C"),
            name: "crate-C".to_string(),
            size: 10,
        };
        let fd4 = FileDesc {
            path: PathBuf::from("crate-D"),
            name: "crate-D".to_string(),
            size: 6,
        };
        let fd5 = FileDesc {
            path: PathBuf::from("crate-E"),
            name: "crate-E".to_string(),
            size: 4,
        };
        let list_fd: Vec<FileDesc> = vec![fd1, fd2, fd3, fd4, fd5];
        let list_cb: Vec<RgSrcInfo> = stats_from_file_desc_list(list_fd);

        let is: String = reg_src_list_to_string(6, list_cb);

        let mut wanted = String::new();
        for i in &[
            "Name    Count Average Total\n",
            "crate-C 1     10 B    10 B\n",
            "crate-D 1     6 B     6 B\n",
            "crate-E 1     4 B     4 B\n",
            "crate-B 1     2 B     2 B\n",
            "crate-A 1     1 B     1 B\n",
        ] {
            wanted.push_str(i);
        }
        assert_eq!(is, wanted);
    }

    #[test]
    fn stats_from_file_desc_same_name_2_one() {
        let fd1 = FileDesc {
            path: PathBuf::from("crate-A"),
            name: "crate-A".to_string(),
            size: 3,
        };
        let fd2 = FileDesc {
            path: PathBuf::from("crate-A"),
            name: "crate-A".to_string(),
            size: 3,
        };

        let list_fd: Vec<FileDesc> = vec![fd1, fd2];
        let list_cb: Vec<RgSrcInfo> = stats_from_file_desc_list(list_fd);
        let is: String = reg_src_list_to_string(2, list_cb);
        let wanted = String::from("Name    Count Average Total\ncrate-A 2     3 B     6 B\n");

        assert_eq!(is, wanted);
    }

    #[test]
    fn stats_from_file_desc_same_name_3_one() {
        let fd1 = FileDesc {
            path: PathBuf::from("crate-A"),
            name: "crate-A".to_string(),
            size: 3,
        };
        let fd2 = FileDesc {
            path: PathBuf::from("crate-A"),
            name: "crate-A".to_string(),
            size: 3,
        };
        let fd3 = FileDesc {
            path: PathBuf::from("crate-A"),
            name: "crate-A".to_string(),
            size: 3,
        };

        let list_fd: Vec<FileDesc> = vec![fd1, fd2, fd3];

        let list_cb: Vec<RgSrcInfo> = stats_from_file_desc_list(list_fd);
        let is: String = reg_src_list_to_string(3, list_cb);
        let wanted = String::from("Name    Count Average Total\ncrate-A 3     3 B     9 B\n");

        assert_eq!(is, wanted);
    }

    #[test]
    fn stats_from_file_desc_same_name_3_one_2() {
        let fd1 = FileDesc {
            path: PathBuf::from("crate-A"),
            name: "crate-A".to_string(),
            size: 2,
        };
        let fd2 = FileDesc {
            path: PathBuf::from("crate-A"),
            name: "crate-A".to_string(),
            size: 4,
        };
        let fd3 = FileDesc {
            path: PathBuf::from("crate-A"),
            name: "crate-A".to_string(),
            size: 12,
        };

        let list_fd: Vec<FileDesc> = vec![fd1, fd2, fd3];
        let list_cb: Vec<RgSrcInfo> = stats_from_file_desc_list(list_fd);
        let is: String = reg_src_list_to_string(3, list_cb);
        let wanted = String::from("Name    Count Average Total\ncrate-A 3     6 B     18 B\n");
        assert_eq!(is, wanted);
    }

    #[test]
    fn stats_from_file_desc_multi() {
        let fd1 = FileDesc {
            path: PathBuf::from("crate-A"),
            name: "crate-A".to_string(),
            size: 2,
        };
        let fd2 = FileDesc {
            path: PathBuf::from("crate-A"),
            name: "crate-A".to_string(),
            size: 4,
        };
        let fd3 = FileDesc {
            path: PathBuf::from("crate-A"),
            name: "crate-A".to_string(),
            size: 12,
        };

        let fd4 = FileDesc {
            path: PathBuf::from("crate-B"),
            name: "crate-B".to_string(),
            size: 2,
        };
        let fd5 = FileDesc {
            path: PathBuf::from("crate-B"),
            name: "crate-B".to_string(),
            size: 8,
        };

        let fd6 = FileDesc {
            path: PathBuf::from("crate-C"),
            name: "crate-C".to_string(),
            size: 0,
        };
        let fd7 = FileDesc {
            path: PathBuf::from("crate-C"),
            name: "crate-C".to_string(),
            size: 100,
        };

        let fd8 = FileDesc {
            path: PathBuf::from("crate-D"),
            name: "crate-D".to_string(),
            size: 1,
        };

        let list_fd: Vec<FileDesc> = vec![fd1, fd2, fd3, fd4, fd5, fd6, fd7, fd8];
        let list_cb: Vec<RgSrcInfo> = stats_from_file_desc_list(list_fd);
        let is: String = reg_src_list_to_string(5, list_cb);

        let mut wanted = String::new();

        for i in &[
            "Name    Count Average Total\n",
            "crate-C 2     50 B    100 B\n",
            "crate-A 3     6 B     18 B\n",
            "crate-B 2     5 B     10 B\n",
            "crate-D 1     1 B     1 B\n",
        ] {
            wanted.push_str(i);
        }
        assert_eq!(is, wanted);
    }
}
#[cfg(all(test, feature = "bench"))]
mod benchmarks {
    use super::*;
    use crate::test::black_box;
    use crate::test::Bencher;

    #[bench]
    fn bench_few(b: &mut Bencher) {
        let fd1 = FileDesc {
            path: PathBuf::from("crate-A"),
            name: "crate-A".to_string(),
            size: 2,
        };
        let fd2 = FileDesc {
            path: PathBuf::from("crate-A"),
            name: "crate-A".to_string(),
            size: 4,
        };
        let fd3 = FileDesc {
            path: PathBuf::from("crate-A"),
            name: "crate-A".to_string(),
            size: 12,
        };

        let fd4 = FileDesc {
            path: PathBuf::from("crate-B"),
            name: "crate-B".to_string(),
            size: 2,
        };
        let fd5 = FileDesc {
            path: PathBuf::from("crate-B"),
            name: "crate-B".to_string(),
            size: 8,
        };

        let fd6 = FileDesc {
            path: PathBuf::from("crate-C"),
            name: "crate-C".to_string(),
            size: 0,
        };
        let fd7 = FileDesc {
            path: PathBuf::from("crate-C"),
            name: "crate-C".to_string(),
            size: 100,
        };

        let fd8 = FileDesc {
            path: PathBuf::from("crate-D"),
            name: "crate-D".to_string(),
            size: 1,
        };

        let list_fd: Vec<FileDesc> = vec![fd1, fd2, fd3, fd4, fd5, fd6, fd7, fd8];

        b.iter(|| {
            let list_fd = list_fd.clone(); // @FIXME  don't?
            let list_cb: Vec<RgSrcInfo> = stats_from_file_desc_list(list_fd);
            let is: String = reg_src_list_to_string(5, list_cb);

            black_box(is);
        });
    }

}
