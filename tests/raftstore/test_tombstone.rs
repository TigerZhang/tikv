// Copyright 2016 PingCAP, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// See the License for the specific language governing permissions and
// limitations under the License.

use std::time::Duration;

use kvproto::raftpb::ConfChangeType;
use kvproto::raft_serverpb;
use tikv::util::HandyRwLock;

use super::cluster::{Cluster, Simulator};
use super::node::new_node_cluster;
use super::server::new_server_cluster;
use super::util::*;

fn test_tombstone<T: Simulator>(cluster: &mut Cluster<T>) {
    // init_env_log();

    let r1 = cluster.bootstrap_conf_change();

    cluster.start();

    // add peer (2,2,2) to region 1.
    cluster.change_peer(r1, ConfChangeType::AddNode, new_peer(2, 2));

    let (key, value) = (b"a1", b"v1");
    cluster.must_put(key, value);
    assert_eq!(cluster.get(key), Some(value.to_vec()));

    let engine_2 = cluster.get_engine(2);
    must_get_equal(&engine_2, b"a1", b"v1");

    let region_status = new_status_request(1, new_peer(2, 2), new_region_leader_cmd());
    let resp = cluster.call_command(region_status, Duration::from_secs(3)).unwrap();
    assert!(resp.get_status_response().has_region_leader());

    // add peer (3, 3, 3) to region 1.
    cluster.change_peer(r1, ConfChangeType::AddNode, new_peer(3, 3));
    // Remove peer (2, 2, 2) from region 1.
    cluster.change_peer(r1, ConfChangeType::RemoveNode, new_peer(2, 2));

    // After new leader is elected, the change peer must be finished.
    cluster.leader_of_region(1).unwrap();
    let (key, value) = (b"a3", b"v3");
    cluster.must_put(key, value);
    assert_eq!(cluster.get(key), Some(value.to_vec()));

    let engine_2 = cluster.get_engine(2);
    must_get_none(&engine_2, b"a1");
    must_get_none(&engine_2, b"a3");

    let epoch = cluster.pd_client
                       .rl()
                       .get_region_by_id(1)
                       .unwrap()
                       .get_region_epoch()
                       .clone();

    // Send a stale raft message to peer (2, 2, 2)
    let mut raft_msg = raft_serverpb::RaftMessage::new();

    raft_msg.set_region_id(1);
    raft_msg.set_from_peer(new_peer(1, 1));
    raft_msg.set_to_peer(new_peer(2, 2));
    raft_msg.set_region_epoch(epoch.clone());

    cluster.send_raft_msg(raft_msg).unwrap();

    // We must get RegionNotFound error.
    let region_status = new_status_request(1, new_peer(2, 2), new_region_leader_cmd());
    let resp = cluster.call_command(region_status, Duration::from_secs(3)).unwrap();
    assert!(resp.get_header().get_error().has_region_not_found(),
            format!("region must not found, but got {:?}", resp));
}

#[test]
fn test_node_tombstone() {
    let count = 5;
    let mut cluster = new_node_cluster(0, count);
    test_tombstone(&mut cluster);
}

#[test]
fn test_server_tombstone() {
    let count = 5;
    let mut cluster = new_server_cluster(0, count);
    test_tombstone(&mut cluster);
}
