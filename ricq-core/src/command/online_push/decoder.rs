use bytes::{Buf, Bytes};
use jcers::Jce;

use crate::command::online_push::{GroupMessagePart, OnlinePushTrans, PushTransInfo, ReqPush};
use crate::common::group_uin2code;
use crate::structs::{GroupDisband, GroupLeave, GroupMemberPermission, MemberPermissionChange};
use crate::{jce, pb, RQError, RQResult};
use prost::Message;

impl super::super::super::Engine {
    // 解析群消息分片 长消息需要合并
    // OnlinePush.PbPushGroupMsg
    pub fn decode_group_message_packet(&self, payload: Bytes) -> RQResult<GroupMessagePart> {
        let message = pb::msg::PushMessagePacket::decode(&*payload)?
            .message
            .ok_or_else(|| RQError::Decode("message is none".to_string()))?;

        let head = message
            .head
            .as_ref()
            .ok_or_else(|| RQError::Decode("head is none".to_string()))?;
        let body = message
            .body
            .as_ref()
            .ok_or_else(|| RQError::Decode("body is none".to_string()))?;
        let content = message
            .content
            .as_ref()
            .ok_or_else(|| RQError::Decode("content is none".to_string()))?;
        let rich_text = body
            .rich_text
            .as_ref()
            .ok_or_else(|| RQError::Decode("rich_text is none".to_string()))?;
        return Ok(GroupMessagePart {
            seq: head
                .msg_seq
                .ok_or_else(|| RQError::Decode("msg_seq is none".to_string()))?,
            rand: rich_text
                .attr
                .as_ref()
                .ok_or_else(|| RQError::Decode("attr is none".into()))?
                .random
                .ok_or_else(|| RQError::Decode("attr.random is none".into()))?,
            group_code: head
                .group_info
                .as_ref()
                .ok_or_else(|| RQError::Decode("group_info is none".into()))?
                .group_code
                .ok_or_else(|| RQError::Decode("group_info.group_code is none".into()))?,
            group_name: String::from_utf8_lossy(
                head.group_info
                    .as_ref()
                    .ok_or_else(|| RQError::Decode("group_info is none".into()))?
                    .group_name
                    .as_ref()
                    .ok_or_else(|| RQError::Decode("group_info.group_name is none".into()))?,
            )
            .to_string(),
            group_card: String::from_utf8_lossy(
                head.group_info
                    .as_ref()
                    .ok_or_else(|| RQError::Decode("group_info is none".into()))?
                    .group_card
                    .as_ref()
                    .ok_or_else(|| RQError::Decode("group_info.group_card is none".into()))?,
            )
            .to_string(),
            from_uin: head
                .from_uin
                .ok_or_else(|| RQError::Decode("from_uin is none".into()))?,
            elems: rich_text.elems.clone(),
            time: head
                .msg_time
                .ok_or_else(|| RQError::Decode("msg_time is none".into()))?,
            pkg_num: content
                .pkg_num
                .ok_or_else(|| RQError::Decode("pkg_num is none".into()))?,
            pkg_index: content
                .pkg_index
                .ok_or_else(|| RQError::Decode("pkg_index is none".into()))?,
            div_seq: content
                .div_seq
                .ok_or_else(|| RQError::Decode("div_seq is none".into()))?,
            ptt: rich_text.ptt.clone(),
        });
    }

    // OnlinePush.ReqPush
    pub fn decode_online_push_req_packet(&self, mut payload: Bytes) -> RQResult<ReqPush> {
        let mut request: jce::RequestPacket = jcers::from_buf(&mut payload)?;
        let mut data: jce::RequestDataVersion2 = jcers::from_buf(&mut request.s_buffer)?;
        let mut req = data
            .map
            .remove("req")
            .ok_or_else(|| RQError::Decode("req is none".into()))?;
        let mut msg = req
            .remove("OnlinePushPack.SvcReqPushMsg")
            .ok_or_else(|| RQError::Decode("OnlinePushPack.SvcReqPushMsg is none".into()))?;
        msg.advance(1);
        let mut jr = Jce::new(&mut msg);
        let uin: i64 = jr.get_by_tag(0)?;
        let msg_infos: Vec<jce::PushMessageInfo> = jr.get_by_tag(2)?;

        Ok(ReqPush { uin, msg_infos })
    }

    pub fn decode_online_push_trans_packet(&self, payload: Bytes) -> RQResult<OnlinePushTrans> {
        let info = pb::msg::TransMsgInfo::decode(&*payload)?;
        let msg_seq = info.msg_seq.unwrap_or_default();
        let msg_uid = info.msg_uid.unwrap_or_default();
        let msg_time = info.msg_time.unwrap_or_default();
        let group_uin = info.from_uin.ok_or_else(|| {
            RQError::Decode("decode_online_push_trans_packet from_uin is 0".to_string())
        })?;
        let mut data = Bytes::from(
            info.msg_data
                .ok_or_else(|| RQError::Decode("msg_data is none".into()))?,
        );
        // 去重暂时不做
        match info.msg_type {
            Some(34) => {
                data.get_i32();
                data.get_u8();
                let target = data.get_u32() as i64;
                let typ = data.get_u8() as i32;
                let operator = data.get_u32() as i64;
                match typ {
                    0x01 | 0x81 => {
                        return Ok(OnlinePushTrans {
                            msg_seq,
                            msg_uid,
                            msg_time,
                            info: PushTransInfo::GroupDisband(GroupDisband {
                                group_code: group_uin2code(group_uin),
                                operator_uin: operator,
                            }),
                        });
                    }
                    0x02 | 0x82 => {
                        return Ok(OnlinePushTrans {
                            msg_seq,
                            msg_uid,
                            msg_time,
                            info: PushTransInfo::MemberLeave(GroupLeave {
                                group_code: group_uin2code(group_uin),
                                member_uin: target,
                                operator_uin: None,
                            }),
                        });
                    }
                    0x03 | 0x83 => {
                        return Ok(OnlinePushTrans {
                            msg_seq,
                            msg_uid,
                            msg_time,
                            info: PushTransInfo::MemberLeave(GroupLeave {
                                group_code: group_uin2code(group_uin),
                                member_uin: target,
                                operator_uin: Some(operator),
                            }),
                        });
                    }
                    _ => {}
                }
            }
            Some(44) => {
                data.advance(5);
                let var4 = data.get_u8() as i32;
                let mut var5: i64 = 0;
                let target = data.get_u32() as i64;
                if var4 != 0 && var4 != 1 {
                    var5 = data.get_u32() as i64;
                }
                if var5 == 0 && data.len() == 1 {
                    let new_permission = if data.get_u8() == 1 {
                        GroupMemberPermission::Administrator
                    } else {
                        GroupMemberPermission::Member
                    };
                    return Ok(OnlinePushTrans {
                        msg_seq,
                        msg_uid,
                        msg_time,
                        info: PushTransInfo::MemberPermissionChange(MemberPermissionChange {
                            group_code: group_uin2code(group_uin),
                            member_uin: target,
                            new_permission,
                        }),
                    });
                }
            }
            _ => {}
        }
        Err(RQError::Decode(format!(
            "decode_online_push_trans_packet unknown error: {:?}",
            info.msg_type
        )))
    }

    // OnlinePush.PbC2CMsgSync
    pub fn decode_c2c_sync_packet(&self, payload: Bytes) -> RQResult<pb::msg::PbPushMsg> {
        pb::msg::PbPushMsg::decode(&*payload).map_err(Into::into)
    }
}
