export TESTTMP=${PWD}
killall josh-proxy >/dev/null 2>&1
killall hyper-cgi-test-server >/dev/null 2>&1

git init --bare ${TESTTMP}/remote/real_repo.git/ 1> /dev/null
git config -f ${TESTTMP}/remote/real_repo.git/config http.receivepack true
git init --bare ${TESTTMP}/remote/real/repo2.git/ 1> /dev/null
git config -f ${TESTTMP}/remote/real/repo2.git/config http.receivepack true
export RUST_LOG=debug

export GIT_CONFIG_NOSYSTEM=1
export JOSH_SERVICE_NAME="josh-proxy-test"

PATH=${TESTDIR}/../../target/debug/:${PATH}
PATH=${TESTDIR}/../../scripts/:${PATH}

GIT_DIR=${TESTTMP}/remote/ GIT_PROJECT_ROOT=${TESTTMP}/remote/ GIT_HTTP_EXPORT_ALL=1 hyper-cgi-test-server\
    --port=8001\
    --dir=${TESTTMP}/remote/\
    --cmd=git\
    --args=http-backend\
    > ${TESTTMP}/hyper-cgi-test-server.out 2>&1 &
echo $! > ${TESTTMP}/server_pid

${TESTDIR}/../../target/debug/josh-proxy\
    --port=8002\
    --local=${TESTTMP}/remote/scratch/\
    --remote=http://localhost:8001\
    > ${TESTTMP}/josh-proxy.out 2>&1 &
echo $! > ${TESTTMP}/proxy_pid

until curl -s http://localhost:8002/
do
    sleep 0.1
done
