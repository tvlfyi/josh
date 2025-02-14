  $ EXTRA_OPTS=--stacked-changes . ${TESTDIR}/setup_test_env.sh
  $ cd ${TESTTMP}

  $ git clone -q http://localhost:8001/real_repo.git 1> /dev/null
  warning: You appear to have cloned an empty repository.
  $ cd real_repo

  $ mkdir sub1
  $ echo contents1 > sub1/file1
  $ git add sub1
  $ git commit -m "add file1" 1> /dev/null
  $ git push 1> /dev/null
  To http://localhost:8001/real_repo.git
   * [new branch]      master -> master

  $ cd ${TESTTMP}

  $ git clone -q http://localhost:8002/real_repo.git:/sub1.git
  $ cd sub1

  $ echo contents2 > file2
  $ git add file2
  $ git commit -m "Change-Id: 1234" 1> /dev/null
  $ echo contents2 > file7
  $ git add file7
  $ git commit -m "Change-Id: foo7" 1> /dev/null
  $ git log --decorate --graph --pretty="%s %d"
  * Change-Id: foo7  (HEAD -> master)
  * Change-Id: 1234 
  * add file1  (origin/master, origin/HEAD)
  $ git push -o author=josh@example.com origin master:refs/for/master
  remote: josh-proxy        
  remote: response from upstream:        
  remote: To http://localhost:8001/real_repo.git        
  remote:  * [new branch]      master -> @heads/master/josh@example.com        
  remote: To http://localhost:8001/real_repo.git        
  remote:  * [new branch]      1234 -> @changes/master/josh@example.com/1234        
  remote: To http://localhost:8001/real_repo.git        
  remote:  * [new branch]      foo7 -> @changes/master/josh@example.com/foo7        
  remote: 
  remote: 
  To http://localhost:8002/real_repo.git:/sub1.git
   * [new reference]   master -> refs/for/master
  $ git push -o author=josh@example.com origin master:refs/for/other_branch
  remote: josh-proxy        
  remote: response from upstream:        
  remote: Reference "refs/heads/other_branch" does not exist on remote.        
  remote: If you want to create it, pass "-o base=<basebranch>" or "-o base=path/to/ref"        
  remote: to specify a base branch/reference.        
  remote: 
  remote: 
  remote: 
  remote: error: hook declined to update refs/for/other_branch        
  To http://localhost:8002/real_repo.git:/sub1.git
   ! [remote rejected] master -> refs/for/other_branch (hook declined)
  error: failed to push some refs to 'http://localhost:8002/real_repo.git:/sub1.git'
  [1]
  $ git push -o base=master -o author=josh@example.com origin master:refs/for/other_branch
  remote: josh-proxy        
  remote: response from upstream:        
  remote: To http://localhost:8001/real_repo.git        
  remote:  * [new branch]      other_branch -> @heads/other_branch/josh@example.com        
  remote: To http://localhost:8001/real_repo.git        
  remote:  * [new branch]      1234 -> @changes/other_branch/josh@example.com/1234        
  remote: To http://localhost:8001/real_repo.git        
  remote:  * [new branch]      foo7 -> @changes/other_branch/josh@example.com/foo7        
  remote: 
  remote: 
  To http://localhost:8002/real_repo.git:/sub1.git
   * [new reference]   master -> refs/for/other_branch
  $ echo contents2 > file3
  $ git add file3
  $ git commit -m "add file3" 1> /dev/null
  $ git push -o author=josh@example.com origin master:refs/for/master
  remote: josh-proxy        
  remote: response from upstream:        
  remote: rejecting to push a3065162ecee0fecc977ec04a275e10b5e15a39c without Change-Id        
  remote: 
  remote: 
  remote: error: hook declined to update refs/for/master        
  To http://localhost:8002/real_repo.git:/sub1.git
   ! [remote rejected] master -> refs/for/master (hook declined)
  error: failed to push some refs to 'http://localhost:8002/real_repo.git:/sub1.git'
  [1]

  $ curl -s http://localhost:8002/flush
  Flushed credential cache
  $ git fetch origin
  From http://localhost:8002/real_repo.git:/sub1
   * [new branch]      @changes/master/josh@example.com/1234 -> origin/@changes/master/josh@example.com/1234
   * [new branch]      @changes/master/josh@example.com/foo7 -> origin/@changes/master/josh@example.com/foo7
   * [new branch]      @changes/other_branch/josh@example.com/1234 -> origin/@changes/other_branch/josh@example.com/1234
   * [new branch]      @changes/other_branch/josh@example.com/foo7 -> origin/@changes/other_branch/josh@example.com/foo7
   * [new branch]      @heads/master/josh@example.com -> origin/@heads/master/josh@example.com
   * [new branch]      @heads/other_branch/josh@example.com -> origin/@heads/other_branch/josh@example.com
  $ git log --decorate --graph --pretty="%s %d"
  * add file3  (HEAD -> master)
  * Change-Id: foo7  (origin/@heads/other_branch/josh@example.com, origin/@heads/master/josh@example.com, origin/@changes/other_branch/josh@example.com/foo7, origin/@changes/master/josh@example.com/foo7)
  * Change-Id: 1234  (origin/@changes/other_branch/josh@example.com/1234, origin/@changes/master/josh@example.com/1234)
  * add file1  (origin/master, origin/HEAD)

  $ cd ${TESTTMP}/real_repo
  $ git fetch origin
  From http://localhost:8001/real_repo
   * [new branch]      @changes/master/josh@example.com/1234 -> origin/@changes/master/josh@example.com/1234
   * [new branch]      @changes/master/josh@example.com/foo7 -> origin/@changes/master/josh@example.com/foo7
   * [new branch]      @changes/other_branch/josh@example.com/1234 -> origin/@changes/other_branch/josh@example.com/1234
   * [new branch]      @changes/other_branch/josh@example.com/foo7 -> origin/@changes/other_branch/josh@example.com/foo7
   * [new branch]      @heads/master/josh@example.com -> origin/@heads/master/josh@example.com
   * [new branch]      @heads/other_branch/josh@example.com -> origin/@heads/other_branch/josh@example.com
  $ git checkout -q heads/master/josh@example.com
  error: pathspec 'heads/master/josh@example.com' did not match any file(s) known to git
  [1]
  $ git log --decorate --graph --pretty="%s %d"
  * add file1  (HEAD -> master, origin/master)

  $ tree
  .
  `-- sub1
      `-- file1
  
  1 directory, 1 file

Make sure all temporary namespace got removed
  $ tree ${TESTTMP}/remote/scratch/real_repo.git/refs/ | grep request_
  [1]

  $ bash ${TESTDIR}/destroy_test_env.sh
  "real_repo.git" = [':/sub1']
  refs
  |-- heads
  |-- josh
  |   |-- filtered
  |   |   `-- real_repo.git
  |   |       `-- %3A%2Fsub1
  |   |           `-- HEAD
  |   `-- upstream
  |       `-- real_repo.git
  |           |-- HEAD
  |           `-- refs
  |               `-- heads
  |                   |-- @changes
  |                   |   |-- master
  |                   |   |   `-- josh@example.com
  |                   |   |       |-- 1234
  |                   |   |       `-- foo7
  |                   |   `-- other_branch
  |                   |       `-- josh@example.com
  |                   |           |-- 1234
  |                   |           `-- foo7
  |                   |-- @heads
  |                   |   |-- master
  |                   |   |   `-- josh@example.com
  |                   |   `-- other_branch
  |                   |       `-- josh@example.com
  |                   `-- master
  |-- namespaces
  `-- tags
  
  19 directories, 9 files

$ cat ${TESTTMP}/josh-proxy.out
$ cat ${TESTTMP}/josh-proxy.out | grep REPO_UPDATE
$ cat ${TESTTMP}/josh-proxy.out | grep "==="
