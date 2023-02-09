from json import dump, load


class ConfigError(Exception):
    pass


class Key:
    def __init__(self, name, secret):
        self.name = name
        self.secret = secret

    @classmethod
    def from_file(cls, filename):
        assert isinstance(filename, str)
        with open(filename, 'r') as f:
            data = load(f)
        return cls(data['name'], data['secret'])


class Committee:
    def __init__(self, names, addresses, transactions_addr, mempool_addr):
        inputs = [names, addresses, transactions_addr, mempool_addr]
        assert all(isinstance(x, list) for x in inputs)
        assert all(isinstance(x, str) for y in inputs for x in y)
        assert len({len(x) for x in inputs}) == 1

        port = 3000
        host = '127.0.0.1'
        self.number_of_nodes = len(addresses)
        number_of_byzantine_nodes = (self.number_of_nodes - 1) / 3
        number_of_honest_nodes = self.number_of_nodes - number_of_byzantine_nodes

        self.number_of_honest_nodes = int(number_of_honest_nodes)
        self.names = names
        self.consensus = addresses
        self.front = addresses
        self.mempool = mempool_addr

        #for i, address in enumerate(addresses):
        self.json = {
            'consensus': self._build_consensus(),
            'mempool': self._build_mempool()
        }
            #primary_addr = {
                #'transactions': f'{host}:{port + i}'
            #}
            #if i < number_of_honest_nodes:
                #self.json['authorities'][address] = {
                    #'stake': 1,
                    #'primary': primary_addr,
                    #'byzantine': False
                #}
            #else:
                #self.json['authorities'][address] = {
                    #'stake': 1,
                    #'primary': primary_addr,
                    #'byzantine': True
                #}'''

        #self.json = {
            #'consensus': self._build_consensus(),
            #'mempool': self._build_mempool()
        #}

    #def primary_addresses(self, faults=0):
        ''' Returns an ordered list of primaries' addresses. '''
        '''assert faults < self.size()
        addresses = []
        good_nodes = self.size() - faults
        for authority in list(self.json['authorities'].values())[:good_nodes]:
            addresses += [authority['primary']['transactions']]
        return addresses'''

    def _build_consensus(self):
        node = {}
        for a, n, f, b in zip(self.consensus, self.names, self.front, range(0, self.number_of_nodes)):
            if b < self.number_of_honest_nodes:
                node[n] = {
                    'name': n,
                    'stake': 1,
                    'address': a,
                    'transactions_address': f,
                    'byzantine': False,
                }
            else:
                node[n] = {
                    'name': n,
                    'stake': 1,
                    'address': a,
                    'transactions_address': f,
                    'byzantine': True,
                }
        return {'authorities': node, 'epoch': 1}

    def _build_mempool(self):
        node = {}
        for n, f, m in zip(self.names, self.front, self.mempool):
            node[n] = {
                'name': n,
                'stake': 1,
                'transactions_address': f,
                'mempool_address': m
            }
        return {'authorities': node, 'epoch': 1}

    def print(self, filename):
        assert isinstance(filename, str)
        with open(filename, 'w') as f:
            dump(self.json, f, indent=4, sort_keys=True)

    def size(self):
        return len(self.json['consensus']['authorities'])

    @classmethod
    def load(cls, filename):
        assert isinstance(filename, str)
        with open(filename, 'r') as f:
            data = load(f)

        consensus_authorities = data['consensus']['authorities'].values()
        mempool_authorities = data['mempool']['authorities'].values()

        names = [x['name'] for x in consensus_authorities]
        consensus_addr = [x['address'] for x in consensus_authorities]
        transactions_addr = [
            x['transactions_address'] for x in mempool_authorities
        ]
        mempool_addr = [x['mempool_address'] for x in mempool_authorities]
        return cls(names, consensus_addr, transactions_addr, mempool_addr)


class LocalCommittee(Committee):
    def __init__(self, names, port):
        assert isinstance(names, list) and all(
            isinstance(x, str) for x in names)
        assert isinstance(port, int)
        size = len(names)
        consensus = [f'127.0.0.1:{port + i}' for i in range(size)]
        front = [f'127.0.0.1:{port + i + size}' for i in range(size)]
        mempool = [f'127.0.0.1:{port + i + 2*size}' for i in range(size)]
        super().__init__(names, consensus, front, mempool)


class NodeParameters:
    def __init__(self, json):
        inputs = []
        try:
            inputs += [json['consensus']['timeout_delay']]
            inputs += [json['consensus']['sync_retry_delay']]
            inputs += [json['mempool']['gc_depth']]
            inputs += [json['mempool']['sync_retry_delay']]
            inputs += [json['mempool']['sync_retry_nodes']]
            inputs += [json['mempool']['batch_size']]
            inputs += [json['mempool']['max_batch_delay']]
        except KeyError as e:
            raise ConfigError(f'Malformed parameters: missing key {e}')

        if not all(isinstance(x, int) for x in inputs):
            raise ConfigError('Invalid parameters type')

        self.timeout_delay = json['consensus']['timeout_delay']
        self.json = json

    def print(self, filename):
        assert isinstance(filename, str)
        with open(filename, 'w') as f:
            dump(self.json, f, indent=4, sort_keys=True)


class BenchParameters:
    def __init__(self, json):
        try:
            nodes = json['nodes']
            nodes = nodes if isinstance(nodes, list) else [nodes]
            if not nodes or any(x <= 1 for x in nodes):
                raise ConfigError('Missing or invalid number of nodes')

            rate = json['rate']
            rate = rate if isinstance(rate, list) else [rate]
            if not rate:
                raise ConfigError('Missing input rate')

            self.nodes = [int(x) for x in nodes]
            self.rate = [int(x) for x in rate]
            self.tx_size = int(json['tx_size'])
            self.faults = int(json['faults'])
            self.duration = int(json['duration'])
            self.runs = int(json['runs']) if 'runs' in json else 1
        except KeyError as e:
            raise ConfigError(f'Malformed bench parameters: missing key {e}')

        except ValueError:
            raise ConfigError('Invalid parameters type')

        if min(self.nodes) <= self.faults:
            raise ConfigError('There should be more nodes than faults')


class PlotParameters:
    def __init__(self, json):
        try:
            nodes = json['nodes']
            nodes = nodes if isinstance(nodes, list) else [nodes]
            if not nodes:
                raise ConfigError('Missing number of nodes')
            self.nodes = [int(x) for x in nodes]

            self.tx_size = int(json['tx_size'])

            faults = json['faults']
            faults = faults if isinstance(faults, list) else [faults]
            self.faults = [int(x) for x in faults] if faults else [0]

            max_lat = json['max_latency']
            max_lat = max_lat if isinstance(max_lat, list) else [max_lat]
            if not max_lat:
                raise ConfigError('Missing max latency')
            self.max_latency = [int(x) for x in max_lat]

        except KeyError as e:
            raise ConfigError(f'Malformed bench parameters: missing key {e}')

        except ValueError:
            raise ConfigError('Invalid parameters type')
