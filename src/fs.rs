use std::collections::HashMap;

use time::Timespec;

use fuse::Filesystem;

enum DirEntry {
    Directory(Directory),
    File(File),
}

struct Directory {
    entries: HashMap<std::ffi::OsString, u64>
}

impl DirEntry {
    fn size(&self) -> u64 {
        match self {
            DirEntry::Directory(_) => 0,
            DirEntry::File(file) => file.bytes.len() as u64,
        }
    }

    fn perms(&self) -> u16 {
        match self {
            DirEntry::Directory(_) => 0o700,
            DirEntry::File(_) => 0o600,
        }
    }

    fn kind(&self) -> fuse::FileType {
        match self {
            DirEntry::Directory(_) => fuse::FileType::Directory,
            DirEntry::File(_) => fuse::FileType::RegularFile,
        }
    }
}

struct File {
    bytes: Vec<u8>
}

pub struct FstnFS {
    nextino: u64,
    inodes: HashMap<u64, DirEntry>,
}

impl Default for FstnFS {
    fn default() -> Self {
        FstnFS {
            nextino: 3,
            inodes: [
                (1, DirEntry::Directory(Directory {
                    entries: [(std::ffi::OsString::from("hello.txt"), 2)].into_iter().collect(),
                })),
                (2, DirEntry::File(File {
                    bytes: b"Hello world".to_vec(),
                }))
            ].into_iter().collect(),
        }
    }
}

impl Filesystem for FstnFS {
    fn lookup(&mut self, _req: &fuse::Request, parent: u64, name: &std::ffi::OsStr, reply: fuse::ReplyEntry) {
        println!("Lookup {:?}", _req);
        if let Some(DirEntry::Directory(directory)) = self.inodes.get(&parent) {
            if let Some((ino, entry)) = directory.entries.get(name).and_then(|ino| self.inodes.get_key_value(ino)) {
                let attr = fuse::FileAttr {
                    ino: *ino,
                    size: entry.size(),
                    blocks: 0,
                    atime: Timespec { sec: 0, nsec: 0 },
                    mtime: Timespec { sec: 0, nsec: 0 },
                    ctime: Timespec { sec: 0, nsec: 0 },
                    crtime: Timespec { sec: 0, nsec: 0 },
                    kind: entry.kind(),
                    perm: entry.perms(),
                    nlink: 2,
                    uid: 1000,
                    gid: 100,
                    rdev: 0,
                    flags: 0,
                };
                reply.entry(&Timespec { sec: 3, nsec: 0 }, &attr, 0);
                return;
            }
        }
        reply.error(libc::ENOENT);
    }

    fn getattr(&mut self, _req: &fuse::Request, ino: u64, reply: fuse::ReplyAttr) {
        println!("Getattr {:?}", _req);
        if let Some(direntry) = self.inodes.get(&ino) {

            let attr = fuse::FileAttr {
                ino,
                size: direntry.size(),
                blocks: 0,
                atime: Timespec { sec: 0, nsec: 0 },
                mtime: Timespec { sec: 0, nsec: 0 },
                ctime: Timespec { sec: 0, nsec: 0 },
                crtime: Timespec { sec: 0, nsec: 0 },
                kind: direntry.kind(),
                perm: direntry.perms(),
                nlink: 2,
                uid: 1000,
                gid: 100,
                rdev: 0,
                flags: 0,
            };
            reply.attr(&Timespec { sec: 3, nsec: 0 }, &attr);
        } else {
           reply.error(libc::ENOENT)
        }
    }

    fn readdir(&mut self, _req: &fuse::Request, ino: u64, _fh: u64, offset: i64, mut reply: fuse::ReplyDirectory) {
        println!("Readdir {:?}", _req);
        if let Some(DirEntry::Directory(directory)) = self.inodes.get(&ino) {
            for (i, entry) in directory.entries.iter().enumerate().skip(offset as usize) {
                if let Some(dirent) = self.inodes.get(entry.1) {
                    reply.add(*entry.1, (i + 1) as i64, dirent.kind(), entry.0);
                } else {
                    reply.error(libc::ENOENT);
                    return;
                }
            }
            reply.ok();
        } else {
            reply.error(libc::ENOENT);
        }
    }

    fn read(&mut self, _req: &fuse::Request, ino: u64, _fh: u64, offset: i64, size: u32, reply: fuse::ReplyData) {
        println!("Read {:?}", ino);
        let offset = offset as usize;
        let size = size as usize;
        if let Some(DirEntry::File(file)) = self.inodes.get(&ino) {
            let size = std::cmp::min(file.bytes.len() - offset, size);
            reply.data(&file.bytes[offset..size]);
        } else {
            reply.error(libc::ENOENT);
        }
    }

    fn write(&mut self, _req: &fuse::Request, ino: u64, _fh: u64, offset: i64, data: &[u8], _flags: u32, reply: fuse::ReplyWrite) {
        println!("Write {:?}", ino);
        if let Some(DirEntry::File(file)) = self.inodes.get_mut(&ino) {
            if data.len() + (offset as usize) > file.bytes.len() {
                file.bytes.resize(offset as usize + data.len(), 0);

            }
            (&mut file.bytes[(offset as usize)..][..data.len()]).clone_from_slice(data);
        }
        reply.written(data.len() as u32)
    }

    fn setattr(&mut self, _req: &fuse::Request, ino: u64, _mode: Option<u32>, _uid: Option<u32>, _gid: Option<u32>, size: Option<u64>, _atime: Option<Timespec>, _mtime: Option<Timespec>, _fh: Option<u64>, _crtime: Option<Timespec>, _chgtime: Option<Timespec>, _bkuptime: Option<Timespec>, _flags: Option<u32>, reply: fuse::ReplyAttr) {
        if let Some(direntry) = self.inodes.get_mut(&ino) {
            if let Some(newsize) = size {
                if let DirEntry::File(file) = direntry {
                    file.bytes.truncate(newsize as usize);
                }
            }

            let attr = fuse::FileAttr {
                ino,
                size: direntry.size(),
                blocks: 0,
                atime: Timespec { sec: 0, nsec: 0 },
                mtime: Timespec { sec: 0, nsec: 0 },
                ctime: Timespec { sec: 0, nsec: 0 },
                crtime: Timespec { sec: 0, nsec: 0 },
                kind: direntry.kind(),
                perm: direntry.perms(),
                nlink: 2,
                uid: 1000,
                gid: 100,
                rdev: 0,
                flags: 0,
            };
            reply.attr(&Timespec { sec: 3, nsec: 0 }, &attr);
        }
    }

    fn create(&mut self, _req: &fuse::Request, parent: u64, name: &std::ffi::OsStr, _mode: u32, _flags: u32, reply: fuse::ReplyCreate) {
        let ino = self.nextino;
        self.nextino += 1;
        let file = DirEntry::File(File {
            bytes: Vec::new(),
        });
        let attr = fuse::FileAttr {
            ino,
            size: file.size(),
            blocks: 0,
            atime: Timespec { sec: 0, nsec: 0 },
            mtime: Timespec { sec: 0, nsec: 0 },
            ctime: Timespec { sec: 0, nsec: 0 },
            crtime: Timespec { sec: 0, nsec: 0 },
            kind: file.kind(),
            perm: file.perms(),
            nlink: 2,
            uid: 1000,
            gid: 100,
            rdev: 0,
            flags: 0,
        };
        self.inodes.insert(ino, file);
        if let Some(DirEntry::Directory(dir)) = self.inodes.get_mut(&parent) {
            dir.entries.insert(name.to_os_string(), ino);
            reply.created(&Timespec { sec: 3, nsec: 0 }, &attr, 0, 0, 0)
        } else {
            reply.error(libc::ENOENT);
        }
    }

    fn mkdir(&mut self, _req: &fuse::Request, _parent: u64, _name: &std::ffi::OsStr, _mode: u32, reply: fuse::ReplyEntry) {
        let ino = self.nextino;
        self.nextino += 1;
        let file = DirEntry::Directory(Directory {
            entries: HashMap::new(),
        });
        let attr = fuse::FileAttr {
            ino,
            size: file.size(),
            blocks: 0,
            atime: Timespec { sec: 0, nsec: 0 },
            mtime: Timespec { sec: 0, nsec: 0 },
            ctime: Timespec { sec: 0, nsec: 0 },
            crtime: Timespec { sec: 0, nsec: 0 },
            kind: file.kind(),
            perm: file.perms(),
            nlink: 2,
            uid: 1000,
            gid: 100,
            rdev: 0,
            flags: 0,
        };
        self.inodes.insert(ino, file);
        if let Some(DirEntry::Directory(dir)) = self.inodes.get_mut(&parent) {
            dir.entries.insert(name.to_os_string(), ino);
            reply.created(&Timespec { sec: 3, nsec: 0 }, &attr, 0, 0, 0)
        } else {
            reply.error(libc::ENOENT);
        }
    }
}
