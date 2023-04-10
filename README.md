# kubempf
Tool to forward and maintain multiple port forwards to kubernetes pods

## Usage

```
Usage: kubempf [OPTIONS] <FORWARD>...

Arguments:
  <FORWARD>...  [[LOCAL_ADDRESS:]LOCAL_PORT:]service:port

Options:
  -c, --context [<CONTEXT>]      Kubernetes Context
  -n, --namespace [<NAMESPACE>]  Kubernetes Namespace
  -h, --help                     Print help
```

### Forwards

Each forward is passed as plain (possitional) argument in the following format

`[[LOCAL_ADDRESS:]LOCAL_PORT:]SERVICE_NAME:SERVICE_OR_POD_PORT`

eg. `kubempf 192.0.2.31:8080:nginx:80` will bind locally to TCP `192.0.2.31:8080` and
forward all traffic to port `80` on one of the pods matching the label selector for the
`nginx` service.

If local address is left off (eg. `kubempf 8080:nginx:80`) the local address will be set
to `127.0.0.1`
If local port is also left off (eg. `kubempf postgresql:5432`) the local port will be set
to the remote port. It is not currenlty possible to use this shorthand with nammed ports.

It is also possible to forward to named ports, such that `kubempf 8080:nginx:http`
will try and find a port named `http` first on the `nginx` serivce, and if that fails
it will then try and find a port named `http` on the pod matched by the services label
selector.

### Arguments

| Short | Long        | Description                                                    |
| ----- | ----------- | -------------------------------------------------------------- |
| -c    | --context   | Name of the context from the kube config to use                |
| -n    | --namespace | Kuberentes namespace to find the services in                   |
