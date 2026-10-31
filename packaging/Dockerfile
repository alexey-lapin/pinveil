# The pinveil-server image: the static musl binary from the release build, nothing else.
#
# Built by the release workflow per architecture from dist/linux_<arch>/pinveil-server, the
# same binary that goes into the release archive. The server listens on 8080. For the
# SQLite store, mount a volume at /data and set PINVEIL_STORE__MODE=sqlite and
# PINVEIL_STORE__PATH=/data/pinveil.db. Always set PINVEIL_PIN__PEPPER.

FROM busybox:1.37.0 AS data
RUN mkdir -p /data && chown 65532:65532 /data

FROM scratch
ARG TARGETARCH
COPY --chmod=0755 dist/linux_${TARGETARCH}/pinveil-server /pinveil-server
# Owned by the runtime user, so a fresh named volume mounted here is writable.
COPY --from=data /data /data
COPY LICENSE NOTICE /licenses/
USER 65532:65532
EXPOSE 8080
ENTRYPOINT ["/pinveil-server"]
CMD ["--listen", "0.0.0.0:8080"]
