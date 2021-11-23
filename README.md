# Makepress API Docker

This repo allows you to dynamically create WordPress instances on a machine running docker.

The api connects to the docker socet available on unix platforms.

The available endpoints are:
```
GET /list - Lists the current statuses of the instances
POST /create/:name - Create a new instance with the given name
```

A dockerfile is provided so that you can run this as a container too.

This repo is intended to work with [nginx-proxy/nginx-proxy](https://github.com/nginx-proxy/nginx-proxy) to automatically resolve the subdoman from the instance name. You can use your own proxy by setting the `MAKEPRESS_PROXY_LABEL` environment variable. Alternatively, you can let the application create one on launch.

## Environment Variables
There are a number of variables able to be used to change the applications behaviour, these are:
- `MAKEPRESS_NETWORK`: The docker network makepress's container will be put on.
- `MAKEPRESS_DB_USERNAME`: The username to use for instance databases.
- `MAKEPRESS_DB_PASSWORD`: The password to use for instance databases.
- `MAKEPRESS_PROXY_LABEL`: The label used to identify the proxy container.
- `MAKEPRESS_DOMAIN`: The domain Makepress containers will be deployed to.
- `MAKEPRESS_CERTS`: The directory in which certificates for the instances are stored in.

## acme-companion

You can use the [nginx-proxy/acme-companion](https://github.com/nginx-proxy/acme-companion) utility to dynamically create certificates for the created instances. You would implement that similar to this `docker-compose` file:

```yaml
version: '2'

services:
  makepress-proxy:
    image: nginxproxy/nginx-proxy
    ports:
      - "80:80"
      - "443:443"
    volumes:
      - certs:/etc/nginx/certs
      - vhost:/etc/nginx/vhost.d
      - html:/usr/share/nginx/html
      - /var/run/docker.sock:/tmp/docker.sock:ro
    networks:
      - makepress-network
  acme-companion:
    image: nginxproxy/acme-companion
    volumes_from:
      - makepress-proxy:rw
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock:ro
      - acme:/etc/acme.sh
    environment:
      - DEFAULT_EMAIL=yuiyukihira1@gmail.com
    networks:
      - makepress-network
  makepress-api:
    image: makepress-api
    ports:
      - "80:80"
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock
    networks:
      - makepress-network
volumes:
  certs: null
  vhost: null
  html: null
  acme: null
networks:
  makepress-network:
    name: prometheus.makepress.network
```