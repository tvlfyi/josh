![Just One Single History](/splash.png)

Combine the advantages of a monorepo with those of multirepo setups by leveraging a
blazingly-fast, incremental, and reversible implementation of git history filtering.

`josh-proxy` can be integrated with any git host:

```
$ docker run -p 8000:8000 -e JOSH_REMOTE=https://github.com -v josh-vol:/data/git joshproject/josh-proxy:latest
```

See [Container options](#container-options) for full list of environment variables.

## Use cases

### Partial cloning

Reduce scope and size of clones by treating subdirectories of the monorepo
as individual repositories.

```
$ git clone http://josh/monorepo.git:/path/to/library.git
```

The partial repo will act as a normal git repository but only contain the files
found in the subdirectory and only commits affecting those files.
The partial repo supports both fetch as well as push operation.

This helps not just to improve performance on the client due to having fewer files in
the tree,
it also enables collaboration on parts of the monorepo with other parties
utilizing git's normal distributed development features.
For example, this makes it easy to mirror just selected parts of your
repo to public github repositories or specific customers.

### Project composition / Workspaces

Simplify code sharing and dependency management. Beyond just subdirectories,
Josh supports filtering, re-mapping and composition of arbitrary virtual repositories
from the content found in the monorepo.

The mapping itself is also stored in the repository and therefore versioned alongside
the code.

<table>
    <thead>
        <tr>
            <th>Central monorepo</th>
            <th>Project workspaces</th>
            <th>workspace.josh file</th>
        </tr>
    </thead>
    <tbody>
        <tr>
            <td rowspan=2><img src="docs/src/img/central.svg?sanitize=true" alt="Folders and files in central.git" /></td>
            <td><img src="docs/src/img/project1.svg?sanitize=true" alt="Folders and files in project1.git" /></td>
            <td>
<pre>
dependencies = :/modules:[
    ::tools/
    ::library1/
]
</pre>
        </tr>
        <tr>
            <td><img src="docs/src/img/project2.svg?sanitize=true" alt="Folders and files in project2.git" /></td>
            <td>
<pre>libs/library1 = :/modules/library1</pre></td>
        </tr>
    </tbody>
</table>

Workspaces act as normal git repos:

```
$ git clone http://josh/central.git:workspace=workspaces/project1.git
```

### Simplified CI/CD

With everything stored in one repo, CI/CD systems only need to look into one source for each particular
deliverable.
However in traditional monorepo environments dependency mangement is handled by the build system.
Build systems are usually tailored to specific languages and need their input already checked
out on the filesystem.
So the question:

> "What deliverables are affected by a given commit and need to be rebuild?"

cannot be answered without cloning the entire repository and understanding how the languages
used handle dependencies.

In particular when using C familiy languages, hidden dependencies on header files are easy to miss.
For this reason limiting the visibility of files to the compiler by sandboxing is pretty much a requirement
for reproducible builds.

With Josh, each deliverable gets it's own virtual git repository with dependencies declared in the `workspace.josh`
file. This means answering the above question becomes as simple as comparing commit ids.
Furthermore due to the tree filtering each build is guaranteed to be perfectly sandboxed
and only sees those parts of the monorepo that have actually been mapped.

This also means the deliverables to be re-build can be determined without cloning any repos like
typically necessary with normal build tools.

### GraphQL API

It is often desireable to access content stored in git without requiring a clone of the repository.
This is usefull for CI/CD systems or web frontends such as dashboards.

Josh exposes a GraphQL API for that purpose. For example, it can be used to find all workspaces currently
present in the tree:

```
query {
  rev(at:"refs/heads/master", filter:"::**/workspace.josh") {
    files { path }
  }
}
```


### Caching proxy

Even without using the more advanced features like partial cloning or workspaces,
`josh-proxy` can act as a cache to reduce traffic between locations or keep your CI from
performing many requests to the main git host.

## FAQ

See [here](https://josh-project.github.io/josh/faq.html)

## Configuration

### Container options

<table>
    <tr>
        <th>
            Variable
        </th>
        <th>
            Meaning
        </th>
    </tr>
    <tr>
        <td>
            <code>JOSH_REMOTE</code>
        </td>
        <td>
            HTTP remote, including protocol.
            Example: <code>https://github.com</code>
        </td>
    </tr>
    <tr>
        <td>
            <code>JOSH_REMOTE_SSH</code>
        </td>
        <td>
            SSH remote, including protocol.
            Example: <code>ssh://git@github.com</code>
        </td>
    </tr>
    <tr>
        <td>
            <code>JOSH_HTTP_PORT</code>
        </td>
        <td>
            HTTP port to listen on.
            Default: 8000
        </td>
    </tr>
    <tr>
        <td>
            <code>JOSH_SSH_PORT</code>
        </td>
        <td>
            SSH port to listen on.
            Default: 8022
        </td>
    </tr>
    <tr>
        <td>
            <code>JOSH_SSH_MAX_STARTUPS</code>
        </td>
        <td>
            Maximum number of concurrent SSH authentication attempts. Default: 16
        </td>
    </tr>
    <tr>
        <td>
            <code>JOSH_SSH_TIMEOUT</code>
        </td>
        <td>
            Timeout, in seconds, for a single request when serving repos over SSH.
            This time should cover fetch from upstream repo, filtering, and serving
            repo to client. Default: 300
        </td>
    </tr>
    <tr>
        <td>
            <code>JOSH_EXTRA_OPTS</code>
        </td>
        <td>
            Extra options passed directly to
            <code>josh-proxy</code> process
        </td>
    </tr>
</table>

### Container volumes

<table>
    <tr>
        <th>
            Volume
        </th>
        <th>
            Purpose
        </th>
    </tr>
    <tr>
        <td>
            <code>/data/git</code>
        </td>
        <td>
            Git cache volume. If this volume is not
            mounted, the cache will be lost every time
            the container is shut down.
        </td>
    </tr>
    <tr>
        <td>
            <code>/data/keys</code>
        </td>
        <td>
            SSH server keys. If this volume is not
            mounted, a new key will be generated on
            each container startup
        </td>
    </tr>
</table>

### Configuring SSH access

Josh supports SSH access (just pull without pushing, for now).
To use SSH, you need to add the following lines to your `~/.ssh/config`:

```
Host your-josh-instance.com
    ForwardAgent yes
    PreferredAuthentications publickey
```

Alternatively, you can pass those options via `GIT_SSH_COMMAND`:

```
GIT_SSH_COMMAND="ssh -o PreferredAuthentications=publickey -o ForwardAgent=yes" git clone ssh://git@your-josh-instance.com/...
```

In other words, you need to ensure SSH agent forwarding is enabled.
