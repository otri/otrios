# SPDX-License-Identifier: Apache-2.0
FROM caddy:2.11.7-alpine@sha256:d8542f48d34a9cf4e4c11a478865229840e87e4c96ea3f439101f31a5d35f75f
# The host listens on 8080. Remove the image's low-port file capability so
# a non-root process can execute with an empty capability bounding set.
RUN setcap -r /usr/bin/caddy
USER 1000:1000
