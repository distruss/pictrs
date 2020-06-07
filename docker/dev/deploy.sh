# To deploy, run ./deploy [tag]
#!/bin/sh
git checkout master

# Creating the new tag
new_tag="$1"

# Changing the docker-compose prod
sed -i "s/asonix\/pictrs:.*/asonix\/pictrs:$new_tag/" ../prod/docker-compose.yml
git add ../prod/docker-compose.yml

# The commit
git commit -m"Version $new_tag"
git tag $new_tag

# Rebuilding docker
docker-compose build
docker tag dev_pictrs:latest asonix/pictrs:x64-$new_tag
docker push asonix/pictrs:x64-$new_tag

# Build for Raspberry Pi / other archs
# TODO

docker manifest push asonix/pictrs:$new_tag

# Push
git push origin $new_tag
git push
