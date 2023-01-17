# Docker compile helper

If you are having problems compiling on your distro (as I was with Arch). You can build a docker container that will compile it.

Build it from the root of the repo):
```
DOCKER_BUILDKIT=1 docker build --file ./docker/Dockerfile --name turbo-resin-dev .
```

Create it (in deamon mode):
```
docker run --name turbo-resin-dev -dit -v $PWD:/code/ turbo-resin-dev
```

Get a shell in the container and compile:
```
docker exec -it turbo-resin-dev bash
make build
```

You can stop and start the container with 
```
docker stop turbo-resin-dev
docker start turbo-resin-dev
```
