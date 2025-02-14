use git2;

use super::*;
use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use tracing::{info, span, Level};

pub type KnownViews = HashMap<String, (git2::Oid, BTreeSet<String>)>;

lazy_static! {
    static ref KNOWN_FILTERS: std::sync::Mutex<KnownViews> =
        std::sync::Mutex::new(std::collections::HashMap::new());
}

pub fn default_from_to(
    repo: &git2::Repository,
    namespace: &str,
    upstream_repo: &str,
    filter_spec: &str,
) -> Vec<(String, String)> {
    let mut refs = vec![];

    for glob in [
        format!("refs/josh/upstream/{}/refs/heads/*", &to_ns(upstream_repo)),
        format!("refs/josh/upstream/{}/refs/tags/*", &to_ns(upstream_repo)),
    ]
    .iter()
    {
        for refname in repo.references_glob(glob).unwrap().names() {
            let refname = refname.unwrap();
            let to_ref = refname.replacen("refs/josh/upstream", "refs/namespaces", 1);
            let to_ref = to_ref.replacen(&to_ns(upstream_repo), namespace, 1);
            refs.push((refname.to_owned(), to_ref.clone()));
        }
    }

    // no need to rember the nop filter since we already keep a reference to
    // the unfiltered branch in refs/josh/upstream
    if filter_spec != ":/" {
        if let Ok(mut known_filters) = KNOWN_FILTERS.try_lock() {
            let known_f = &mut known_filters
                .entry(upstream_repo.trim_start_matches('/').to_string())
                .or_insert_with(|| (git2::Oid::zero(), BTreeSet::new()));

            known_f.1.insert(filter_spec.to_string());
        }
    }

    refs
}

pub fn memorize_from_to(
    repo: &git2::Repository,
    namespace: &str,
    upstream_repo: &str,
) -> Vec<(String, String)> {
    let mut refs = vec![];
    let glob = format!("refs/josh/upstream/{}/HEAD", &to_ns(upstream_repo));
    for refname in repo.references_glob(&glob).unwrap().names() {
        let refname = refname.unwrap();
        let to_ref = format!("refs/{}/HEAD", &namespace);

        refs.push((refname.to_owned(), to_ref.clone()));
    }

    refs
}

fn run_command(path: &Path, cmd: &str) -> String {
    let shell = shell::Shell {
        cwd: path.to_owned(),
    };

    let output = "";

    let (stdout, stderr, _) = shell.command(cmd);
    let output = format!(
        "{}\n\n{}:\nstdout:\n{}\n\nstderr:{}\n",
        output, cmd, stdout, stderr
    );

    output
}

regex_parsed!(
    UpstreamRef,
    r"refs/josh/upstream/(?P<ns>.*[.]git)/refs/heads/.*",
    [ns]
);

regex_parsed!(
    FilteredRefRegex,
    r"josh/filtered/(?P<upstream_repo>[^/]*[.]git)/(?P<filter_spec>[^/]*)/.*",
    [upstream_repo, filter_spec]
);

/**
 * Determine filter specs that are either likely to be requested and/or
 * expensive to build from scratch using heuristics.
 */
pub fn discover_filter_candidates(transaction: &cache::Transaction) -> JoshResult<()> {
    let repo = transaction.repo();
    let mut known_filters = KNOWN_FILTERS.lock()?;
    let trace_s = span!(Level::TRACE, "discover_filter_candidates");
    let _e = trace_s.enter();

    let refname = "refs/josh/upstream/*.git/refs/heads/*".to_string();

    for reference in repo.references_glob(&refname)? {
        let r = reference?;
        let name = r.name().ok_or(josh_error("reference without name"))?;
        let name = UpstreamRef::from_str(name)
            .ok_or(josh_error("not a ns"))?
            .ns;
        let name = from_ns(&name);
        tracing::trace!("find: {}", name);

        let known_f = &mut known_filters
            .entry(name.clone())
            .or_insert_with(|| (git2::Oid::zero(), BTreeSet::new()));

        if let Some(target) = r.target() {
            if known_f.0 != target {
                let hs = find_all_workspaces_and_subdirectories(&r.peel_to_tree()?)?;
                known_f.0 = target;
                for i in hs {
                    known_f.1.insert(i);
                }
            }
        }
    }

    let refname = "josh/filtered/*.git/*/refs/heads/*".to_string();
    for reference in repo.references_glob(&refname)? {
        let r = reference?;
        let name = r.name().ok_or(josh_error("reference without name"))?;
        tracing::trace!("known: {}", name);
        let filtered = FilteredRefRegex::from_str(name).ok_or(josh_error("not a ns"))?;

        known_filters
            .entry(from_ns(&filtered.upstream_repo))
            .or_insert_with(|| (git2::Oid::zero(), BTreeSet::new()))
            .1
            .insert(from_ns(&filtered.filter_spec));
    }

    Ok(())
}

pub fn find_all_workspaces_and_subdirectories(
    tree: &git2::Tree,
) -> JoshResult<std::collections::HashSet<String>> {
    let _trace_s = span!(Level::TRACE, "find_all_workspaces_and_subdirectories");
    let mut hs = std::collections::HashSet::new();
    tree.walk(git2::TreeWalkMode::PreOrder, |root, entry| {
        if entry.name() == Some("workspace.josh") {
            hs.insert(format!(":workspace={}", root.trim_matches('/')));
        }
        if root.is_empty() {
            return 0;
        }
        let v = format!(":/{}", root.trim_matches('/'));
        if v.chars().filter(|x| *x == '/').count() < 3 {
            hs.insert(v);
        }

        0
    })?;
    Ok(hs)
}

pub fn get_info(
    transaction: &cache::Transaction,
    filter: filter::Filter,
    headref: &str,
) -> JoshResult<String> {
    let _trace_s = span!(Level::TRACE, "get_info");

    let obj = transaction
        .repo()
        .revparse_single(&transaction.refname(headref))?;

    let commit = obj.peel_to_commit()?;

    let mut meta = std::collections::HashMap::new();
    meta.insert("sha1".to_owned(), "".to_owned());
    let filtered = filter::apply_to_commit(filter, &commit, transaction)?;

    let parent_ids = |commit: &git2::Commit| {
        commit
            .parent_ids()
            .map(|x| {
                json!({
                    "commit": x.to_string(),
                    "tree": transaction.repo().find_commit(x)
                        .map(|c| { c.tree_id() })
                        .unwrap_or(git2::Oid::zero())
                        .to_string(),
                })
            })
            .collect::<Vec<_>>()
    };

    let t = if let Ok(filtered) = transaction.repo().find_commit(filtered) {
        json!({
            "commit": filtered.id().to_string(),
            "tree": filtered.tree_id().to_string(),
            "parents": parent_ids(&filtered),
        })
    } else {
        json!({
            "commit": git2::Oid::zero().to_string(),
            "tree": git2::Oid::zero().to_string(),
            "parents": json!([]),
        })
    };

    let s = json!({
        "commit": commit.id().to_string(),
        "tree": commit.tree_id().to_string(),
        "parents": parent_ids(&commit),
        "filtered": t,
    });

    Ok(serde_json::to_string(&s)?)
}

#[tracing::instrument(skip(transaction))]
pub fn refresh_known_filters(transaction: &cache::Transaction) -> JoshResult<usize> {
    let known_filters = KNOWN_FILTERS.lock()?;
    for (upstream_repo, e) in known_filters.iter() {
        let t = transaction.try_clone()?;
        info!("background rebuild root: {:?}", upstream_repo);

        for filter_spec in e.1.iter() {
            tracing::trace!("background rebuild: {:?} {:?}", upstream_repo, filter_spec);

            let refs = memorize_from_to(
                t.repo(),
                &to_filtered_ref(upstream_repo, filter_spec),
                upstream_repo,
            );

            filter_refs(&t, filter::parse(filter_spec)?, &refs, filter::empty(), "")?;
        }
    }
    Ok(0)
}

pub fn get_known_filters(
) -> JoshResult<std::collections::BTreeMap<String, std::collections::BTreeSet<String>>> {
    Ok(KNOWN_FILTERS
        .lock()?
        .iter()
        .map(|(repo, (_, filters))| (repo.clone(), filters.clone()))
        .collect())
}

pub fn run(repo_path: &std::path::Path, do_gc: bool) -> JoshResult<()> {
    let transaction = cache::Transaction::open(repo_path, None)?;
    housekeeping::discover_filter_candidates(&transaction)?;
    refresh_known_filters(&transaction)?;
    info!(
        "{}",
        run_command(transaction.repo().path(), "git count-objects -v").replace("\n", "  ")
    );
    if do_gc {
        info!(
            "\n----------\n{}\n----------",
            run_command(transaction.repo().path(), "git repack -adkbn --threads=1")
        );
        info!(
            "\n----------\n{}\n----------",
            run_command(transaction.repo().path(), "git count-objects -vH")
        );
        info!(
            "\n----------\n{}\n----------",
            run_command(transaction.repo().path(), "git prune --expire=2w")
        );
    }
    Ok(())
}
