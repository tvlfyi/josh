pub mod auth;
pub mod juniper_hyper;

#[macro_use]
extern crate lazy_static;

fn baseref_and_options(refname: &str) -> josh::JoshResult<(String, String, Vec<String>, bool)> {
    let mut split = refname.splitn(2, '%');
    let push_to = split.next().ok_or(josh::josh_error("no next"))?.to_owned();

    let options = if let Some(options) = split.next() {
        options.split(',').map(|x| x.to_string()).collect()
    } else {
        vec![]
    };

    let mut baseref = push_to.to_owned();
    let mut for_review = false;

    if baseref.starts_with("refs/for") {
        for_review = true;
        baseref = baseref.replacen("refs/for", "refs/heads", 1)
    }
    if baseref.starts_with("refs/drafts") {
        baseref = baseref.replacen("refs/drafts", "refs/heads", 1)
    }
    Ok((baseref, push_to, options, for_review))
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct RepoUpdate {
    pub refs: std::collections::HashMap<String, (String, String)>,
    pub remote_url: String,
    pub auth: auth::Handle,
    pub port: String,
    pub filter_spec: String,
    pub base_ns: String,
    pub git_ns: String,
    pub git_dir: String,
    pub stacked_changes: bool,
    pub context_propagator: std::collections::HashMap<String, String>,
}

pub fn process_repo_update(repo_update: RepoUpdate) -> josh::JoshResult<String> {
    let p = std::path::PathBuf::from(&repo_update.git_dir)
        .join("refs/namespaces")
        .join(&repo_update.git_ns)
        .join("push_options");

    let push_options_string = std::fs::read_to_string(p)?;
    let push_options: std::collections::HashMap<String, String> =
        serde_json::from_str(&push_options_string)?;

    for (refname, (old, new)) in repo_update.refs.iter() {
        tracing::debug!("REPO_UPDATE env ok");

        let transaction = josh::cache::Transaction::open(
            std::path::Path::new(&repo_update.git_dir),
            Some(&format!("refs/josh/upstream/{}/", repo_update.base_ns)),
        )?;

        let old = git2::Oid::from_str(old)?;

        let (baseref, push_to, options, for_review) = baseref_and_options(refname)?;
        let josh_merge = push_options.contains_key("merge");

        tracing::debug!("push options: {:?}", push_options);
        tracing::debug!("josh-merge: {:?}", josh_merge);

        let old = if old == git2::Oid::zero() {
            let rev = format!("refs/namespaces/{}/{}", repo_update.git_ns, &baseref);
            let oid = if let Ok(x) = transaction.repo().revparse_single(&rev) {
                x.id()
            } else {
                old
            };
            tracing::debug!("push: old oid: {:?}, rev: {:?}", oid, rev);
            oid
        } else {
            tracing::debug!("push: old oid: {:?}, refname: {:?}", old, refname);
            old
        };

        let original_target_ref = if let Some(base) = push_options.get("base") {
            // Allow user to use just the branchname as the base:
            let full_path_base_refname = transaction.refname(&format!("refs/heads/{}", base));
            if transaction
                .repo()
                .refname_to_id(&full_path_base_refname)
                .is_ok()
            {
                full_path_base_refname
            } else {
                transaction.refname(base)
            }
        } else {
            transaction.refname(&baseref)
        };

        let original_target =
            if let Ok(oid) = transaction.repo().refname_to_id(&original_target_ref) {
                tracing::debug!(
                    "push: original_target oid: {:?}, original_target_ref: {:?}",
                    oid,
                    original_target_ref
                );
                oid
            } else {
                return Err(josh::josh_error(&unindent::unindent(&format!(
                    r###"
                    Reference {:?} does not exist on remote.
                    If you want to create it, pass "-o base=<basebranch>" or "-o base=path/to/ref"
                    to specify a base branch/reference.
                    "###,
                    baseref
                ))));
            };

        let reparent_orphans = if push_options.contains_key("create") {
            Some(original_target)
        } else {
            None
        };

        let stacked_changes = repo_update.stacked_changes && for_review;

        let mut change_ids = if stacked_changes { Some(vec![]) } else { None };

        let filterobj = josh::filter::parse(&repo_update.filter_spec)?;
        let new_oid = git2::Oid::from_str(new)?;
        let backward_new_oid = {
            tracing::debug!("=== MORE");

            tracing::debug!("=== processed_old {:?}", old);

            match josh::history::unapply_filter(
                &transaction,
                filterobj,
                original_target,
                old,
                new_oid,
                josh_merge,
                reparent_orphans,
                &mut change_ids,
            )? {
                josh::UnapplyResult::Done(rewritten) => {
                    tracing::debug!("rewritten");
                    rewritten
                }
                josh::UnapplyResult::BranchDoesNotExist => {
                    return Err(josh::josh_error("branch does not exist on remote"));
                }
                josh::UnapplyResult::RejectMerge(msg) => {
                    return Err(josh::josh_error(&msg));
                }
                josh::UnapplyResult::RejectAmend(msg) => {
                    return Err(josh::josh_error(&format!(
                        "rejecting to amend {:?} with conflicting changes",
                        msg
                    )));
                }
            }
        };

        let oid_to_push = if josh_merge {
            let backward_commit = transaction.repo().find_commit(backward_new_oid)?;
            if let Ok(Ok(base_commit)) = transaction
                .repo()
                .revparse_single(&original_target_ref)
                .map(|x| x.peel_to_commit())
            {
                let merged_tree = transaction
                    .repo()
                    .merge_commits(&base_commit, &backward_commit, None)?
                    .write_tree_to(transaction.repo())?;
                transaction.repo().commit(
                    None,
                    &backward_commit.author(),
                    &backward_commit.committer(),
                    &format!("Merge from {}", &repo_update.filter_spec),
                    &transaction.repo().find_tree(merged_tree)?,
                    &[&base_commit, &backward_commit],
                )?
            } else {
                return Err(josh::josh_error("josh_merge failed"));
            }
        } else {
            backward_new_oid
        };

        let ref_with_options = if !options.is_empty() {
            format!("{}{}{}", push_to, "%", options.join(","))
        } else {
            push_to
        };

        let author = if let Some(p) = push_options.get("author") {
            p.to_string()
        } else {
            "".to_string()
        };

        let to_push = if let Some(change_ids) = change_ids {
            let mut v = vec![(
                format!(
                    "refs/heads/@heads/{}/{}",
                    baseref.replacen("refs/heads/", "", 1),
                    author,
                ),
                oid_to_push,
                baseref.replacen("refs/heads/", "", 1),
            )];
            v.append(&mut change_ids_to_refs(baseref, author, change_ids)?);
            v
        } else {
            vec![(ref_with_options, oid_to_push, "JOSH_PUSH".to_string())]
        };

        let mut resp = vec![];

        for (reference, oid, display_name) in to_push {
            let (text, status) = push_head_url(
                transaction.repo(),
                oid,
                &reference,
                &repo_update.remote_url,
                &repo_update.auth,
                &repo_update.git_ns,
                &display_name,
                stacked_changes,
            )?;
            if status != 0 {
                return Err(josh::josh_error(&text));
            }

            resp.push(text.to_string());

            let mut warnings = josh::filter::compute_warnings(
                &transaction,
                filterobj,
                transaction.repo().find_commit(oid)?.tree()?,
            );

            if !warnings.is_empty() {
                resp.push("warnings:".to_string());
                resp.append(&mut warnings);
            }
        }

        let reapply = josh::filter::apply_to_commit(
            filterobj,
            &transaction.repo().find_commit(oid_to_push)?,
            &transaction,
        )?;

        if new_oid != reapply {
            transaction.repo().reference(
                &format!(
                    "refs/josh/rewrites/{}/{:?}/r_{}",
                    repo_update.base_ns,
                    filterobj.id(),
                    reapply
                ),
                reapply,
                true,
                "reapply",
            )?;
            let text = format!("REWRITE({} -> {})", new_oid, reapply);
            tracing::debug!("{}", text);
            resp.push(text);
        }

        return Ok(resp.join("\n"));
    }

    Ok("".to_string())
}

pub fn push_head_url(
    repo: &git2::Repository,
    oid: git2::Oid,
    refname: &str,
    url: &str,
    auth: &auth::Handle,
    namespace: &str,
    display_name: &str,
    force: bool,
) -> josh::JoshResult<(String, i32)> {
    let rn = format!("refs/{}", &namespace);

    let spec = format!("{}:{}", &rn, &refname);

    let shell = josh::shell::Shell {
        cwd: repo.path().to_owned(),
    };
    let (username, password) = auth.parse()?;
    let cmd = format!(
        "git push {} {} '{}'",
        if force { "-f" } else { "" },
        &url,
        &spec
    );
    let mut fakehead = repo.reference(&rn, oid, true, "push_head_url")?;
    let (stdout, stderr, status) = shell.command_env(
        &cmd,
        &[],
        &[("GIT_PASSWORD", &password), ("GIT_USER", &username)],
    );
    fakehead.delete()?;
    tracing::debug!("{}", &stderr);
    tracing::debug!("{}", &stdout);

    let stderr = stderr.replace(&rn, display_name);

    Ok((stderr, status))
}

pub fn create_repo(path: &std::path::Path) -> josh::JoshResult<()> {
    tracing::debug!("init base repo: {:?}", path);
    std::fs::create_dir_all(path).expect("can't create_dir_all");
    git2::Repository::init_bare(path)?;
    let shell = josh::shell::Shell {
        cwd: path.to_path_buf(),
    };
    shell.command("git config http.receivepack true");
    shell.command("git config uploadpack.allowsidebandall true");
    shell.command("git config user.name josh");
    shell.command("git config user.email josh@localhost");
    shell.command("git config uploadpack.allowAnySHA1InWant true");
    shell.command("git config uploadpack.allowReachableSHA1InWant true");
    shell.command("git config uploadpack.allowTipSha1InWant true");
    shell.command("git config receive.advertisePushOptions true");
    let ce = std::env::current_exe().expect("can't find path to exe");
    shell.command("rm -Rf hooks");
    shell.command("mkdir hooks");
    std::os::unix::fs::symlink(ce.clone(), path.join("hooks").join("update"))
        .expect("can't symlink update hook");
    std::os::unix::fs::symlink(ce, path.join("hooks").join("pre-receive"))
        .expect("can't symlink pre-receive hook");
    shell.command(
        &"git config credential.helper '!f() { echo \"username=\"$GIT_USER\"\npassword=\"$GIT_PASSWORD\"\"; }; f'"
            .to_string(),
    );
    shell.command("git config gc.auto 0");
    shell.command("git pack-refs --all");

    if std::env::var_os("JOSH_KEEP_NS") == None {
        std::fs::remove_dir_all(path.join("refs/namespaces")).ok();
    }
    tracing::info!("repo initialized");
    Ok(())
}

pub fn get_head(
    path: &std::path::Path,
    url: &str,
    auth: &auth::Handle,
) -> josh::JoshResult<String> {
    let shell = josh::shell::Shell {
        cwd: path.to_owned(),
    };
    let (username, password) = auth.parse()?;

    let cmd = format!("git ls-remote --symref {} {}", &url, "HEAD");
    tracing::info!("get_head {:?} {:?} {:?}", cmd, path, "");

    let (stdout, _stderr, _) = shell.command_env(
        &cmd,
        &[],
        &[("GIT_PASSWORD", &password), ("GIT_USER", &username)],
    );

    let head = stdout
        .lines()
        .next()
        .unwrap_or("refs/heads/master")
        .to_string();

    let head = head.replacen("ref: ", "", 1);
    let head = head.replacen("\tHEAD", "", 1);

    Ok(head)
}

pub fn fetch_refs_from_url(
    path: &std::path::Path,
    upstream_repo: &str,
    url: &str,
    refs_prefixes: &[String],
    auth: &auth::Handle,
) -> josh::JoshResult<bool> {
    let specs: Vec<_> = refs_prefixes
        .iter()
        .map(|r| {
            format!(
                "'+{}:refs/josh/upstream/{}/{}'",
                &r,
                josh::to_ns(upstream_repo),
                &r
            )
        })
        .collect();

    let shell = josh::shell::Shell {
        cwd: path.to_owned(),
    };

    let cmd = format!("git fetch --prune --no-tags {} {}", &url, &specs.join(" "));
    tracing::info!("fetch_refs_from_url {:?} {:?} {:?}", cmd, path, "");

    let (username, password) = auth.parse()?;
    let (_stdout, stderr, _) = shell.command_env(
        &cmd,
        &[],
        &[("GIT_PASSWORD", &password), ("GIT_USER", &username)],
    );
    tracing::debug!("fetch_refs_from_url done {:?} {:?} {:?}", cmd, path, stderr);
    if stderr.contains("fatal: Authentication failed") {
        return Ok(false);
    }
    if stderr.contains("fatal:") {
        tracing::error!("{:?}", stderr);
        return Err(josh::josh_error(&format!("git error: {:?}", stderr)));
    }
    if stderr.contains("error:") {
        tracing::error!("{:?}", stderr);
        return Err(josh::josh_error(&format!("git error: {:?}", stderr)));
    }
    Ok(true)
}

pub struct TmpGitNamespace {
    name: String,
    repo_path: std::path::PathBuf,
    _span: tracing::Span,
}

impl TmpGitNamespace {
    pub fn new(repo_path: &std::path::Path, span: tracing::Span) -> TmpGitNamespace {
        let n = format!("request_{}", uuid::Uuid::new_v4());
        let n2 = n.clone();
        TmpGitNamespace {
            name: n,
            repo_path: repo_path.to_owned(),
            _span: tracing::span!(
                parent: span,
                tracing::Level::TRACE,
                "TmpGitNamespace",
                name = n2.as_str(),
            ),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn reference(&self, refname: &str) -> String {
        return format!("refs/namespaces/{}/{}", &self.name, refname);
    }
}

impl std::fmt::Debug for TmpGitNamespace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JoshProxyService")
            .field("repo_path", &self.repo_path)
            .field("name", &self.name)
            .finish()
    }
}

impl Drop for TmpGitNamespace {
    fn drop(&mut self) {
        if std::env::var_os("JOSH_KEEP_NS") != None {
            return;
        }
        let request_tmp_namespace = self.repo_path.join("refs/namespaces").join(&self.name);
        std::fs::remove_dir_all(&request_tmp_namespace).unwrap_or_else(|e| {
            tracing::error!(
                "remove_dir_all {:?} failed, error:{:?}",
                request_tmp_namespace,
                e
            )
        });
    }
}

fn change_ids_to_refs(
    baseref: String,
    change_author: String,
    change_ids: Vec<josh::Change>,
) -> josh::JoshResult<Vec<(String, git2::Oid, String)>> {
    let mut seen = vec![];
    let mut change_ids = change_ids;
    change_ids.retain(|change| change.author == change_author);
    if !change_author.contains('@') {
        return Err(josh::josh_error(
            "Push option 'author' needs to be set to a valid email address",
        ));
    };

    for change in change_ids.iter() {
        if let Some(id) = &change.id {
            if id.contains('@') {
                return Err(josh::josh_error("Change-Id must not contain '@'"));
            }
            if seen.contains(&id) {
                return Err(josh::josh_error(&format!(
                    "rejecting to push {:?} with duplicate Change-Id",
                    change.commit
                )));
            }
            seen.push(&id);
        } else {
            return Err(josh::josh_error(&format!(
                "rejecting to push {:?} without Change-Id",
                change.commit
            )));
        }
    }

    Ok(change_ids
        .iter()
        .map(|change| {
            (
                format!(
                    "refs/heads/@changes/{}/{}/{}",
                    baseref.replacen("refs/heads/", "", 1),
                    change.author,
                    change.id.as_ref().unwrap_or(&"".to_string()),
                ),
                change.commit,
                change
                    .id
                    .as_ref()
                    .unwrap_or(&"JOSH_PUSH".to_string())
                    .to_string(),
            )
        })
        .collect())
}
