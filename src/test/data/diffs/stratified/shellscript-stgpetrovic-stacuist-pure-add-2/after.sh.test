 #!/bin/bash
 
# Compile the app and bundle the resources.
CC="/usr/bin/clang++" BAZEL_CXXOPTS="-std=c++17" bazel build -c opt //:stacuist_webapp --strategy=Genrule=local

ssh root@159.89.213.201 'cd ~/stacuist.com && rm -rf *'
rsync bazel-bin/stacuist.com.tar root@159.89.213.201:~/stacuist.com/
ssh root@159.89.213.201 'cd ~/stacuist.com && tar -xvf stacuist.com.tar'
