<script setup lang="ts">
import { computed } from "vue";
import { send } from "../api";
import { fileUrl } from "../files";
import { showError, state } from "../store";
import Avatar from "../components/Avatar.vue";

const p = computed(() => state.myProfile);
const bg = computed(() => fileUrl(p.value?.background_file_id));
const isStudent = computed(() => state.user?.role === "student");
const targetClasses = computed(() =>
  (state.bootstrap?.classes ?? []).filter((c) => c.id !== state.user?.class_id),
);

function statusText(s: string): string {
  return (
    {
      pending: "待审批",
      out_approved: "转出班已同意",
      in_approved: "转入班已同意",
      approved: "已通过",
      rejected: "已拒绝",
    }[s] ?? s
  );
}

function submitTransfer() {
  if (state.transferToClassId == null) {
    showError("请选择目标班级");
    return;
  }
  send({
    type: "CreateTransfer",
    data: { to_class_id: state.transferToClassId, reason: state.transferReason },
  });
}

const roleText = computed(() => {
  const r = state.user?.role;
  return r === "root" ? "总管理员" : r === "class_admin" ? "班级管理员" : "学生";
});

function pick(kind: "avatar" | "background", e: Event) {
  const input = e.target as HTMLInputElement;
  const f = input.files?.[0];
  if (!f) return;
  if (f.size > 8 * 1024 * 1024) {
    showError("图片最大 8 MB");
    return;
  }
  const reader = new FileReader();
  reader.onload = () => {
    send({ type: "UploadImage", data: { data: String(reader.result ?? ""), kind } });
  };
  reader.readAsDataURL(f);
  input.value = "";
}

function save() {
  const name = state.profileDisplayName.trim();
  if (!name) {
    showError("用户名不能为空");
    return;
  }
  send({
    type: "UpdateProfile",
    data: {
      display_name: name,
      signature: state.profileSignature,
      bio: state.profileBio,
      gender: state.profileGender,
      birthday: state.profileBirthday,
      socials: state.profileSocials,
      public_gender: state.profileShowGender ? state.profileGender : "",
      public_birthday: state.profileShowBirthday ? state.profileBirthday : "",
      public_socials: state.profileShowSocials ? state.profileSocials : "",
    },
  });
}

function back() {
  state.page = "Main";
  state.tab = "me";
}

function unblock(uid: number) {
  send({ type: "UnblockUser", data: { uid } });
}
</script>

<template>
  <div class="page">
    <div
      class="profile-bg"
      :style="bg ? { backgroundImage: `url(${bg})` } : {}"
    >
      <div class="profile-bg-actions">
        <label class="upload-btn">
          更换背景
          <input type="file" accept="image/*" hidden @change="pick('background', $event)" />
        </label>
      </div>
      <div class="profile-head">
        <Avatar :file-id="p?.avatar_file_id" :name="p?.display_name || state.user?.display_name" :size="64" />
        <div class="grow">
          <div class="profile-name">{{ p?.display_name || state.user?.display_name }}</div>
          <div class="account-sub">
            UID {{ p?.uid }} · @{{ p?.handle }} · {{ roleText }}
          </div>
          <div class="account-sub">
            登录方式：{{ state.user?.login_policy === "pin_allowed" ? "PIN 或密码" : "仅密码" }}
          </div>
        </div>
        <label class="upload-btn">
          换头像
          <input type="file" accept="image/*" hidden @change="pick('avatar', $event)" />
        </label>
      </div>
    </div>

    <h3>编辑资料</h3>
    <label>用户名（所有同学可见）</label>
    <input v-model="state.profileDisplayName" maxlength="20" />

    <label>个性签名（所有人可见）</label>
    <input v-model="state.profileSignature" maxlength="100" placeholder="一句话介绍自己" />

    <label>个人简介（所有人可见）</label>
    <textarea v-model="state.profileBio" maxlength="1000" placeholder="兴趣爱好、想说的话……"></textarea>

    <div class="sep"></div>
    <h3>隐私信息</h3>
    <div class="hint">
      下面三项默认不公开。勾选「在资料页展示」后，其他同学才能在你的资料页看到。
    </div>

    <label>性别</label>
    <input v-model="state.profileGender" maxlength="16" placeholder="男 / 女 / 不填" />
    <label class="check-row">
      <input v-model="state.profileShowGender" type="checkbox" />
      <span>在资料页展示性别</span>
    </label>

    <label>生日</label>
    <input v-model="state.profileBirthday" maxlength="32" placeholder="例如 2008-06-01" />
    <label class="check-row">
      <input v-model="state.profileShowBirthday" type="checkbox" />
      <span>在资料页展示生日</span>
    </label>

    <label>社交账号</label>
    <textarea
      v-model="state.profileSocials"
      maxlength="2000"
      placeholder="QQ、微信等，例如：QQ 123456"
    ></textarea>
    <label class="check-row">
      <input v-model="state.profileShowSocials" type="checkbox" />
      <span>在资料页展示社交账号</span>
    </label>

    <button class="primary" @click="save">保存资料</button>
    <button @click="state.page = 'Security'">密码与 PIN 安全设置</button>

    <div class="sep"></div>
    <h3>黑名单</h3>
    <div v-if="state.blocks.length === 0" class="empty">没有拉黑任何人</div>
    <div v-for="b in state.blocks" :key="b.uid" class="friend-row">
      <Avatar :file-id="b.avatar_file_id" :name="b.display_name" />
      <div class="grow">
        <div class="account-name">{{ b.display_name }}</div>
        <div class="account-sub">@{{ b.handle }} · UID {{ b.uid }}</div>
      </div>
      <button class="fixed" @click="unblock(b.uid)">解除</button>
    </div>

    <template v-if="isStudent">
      <div class="sep"></div>
      <h3>转班申请</h3>
      <div v-if="state.myTransfers.length > 0" class="card">
        <div v-for="t in state.myTransfers" :key="t.id" class="small" style="padding: 3px 0">
          {{ t.from_class_name }} → {{ t.to_class_name }} · {{ statusText(t.status) }}
        </div>
      </div>
      <label>目标班级</label>
      <select v-model.number="state.transferToClassId">
        <option :value="null">选择班级</option>
        <option v-for="c in targetClasses" :key="c.id" :value="c.id">
          {{ c.name }} {{ c.grade }}
        </option>
      </select>
      <label>理由（可选）</label>
      <input v-model="state.transferReason" maxlength="500" placeholder="为什么想转班" />
      <button @click="submitTransfer">提交转班申请</button>
      <div class="hint">需要转出班级和转入班级的管理员都同意；总管理员可一次批准。</div>
    </template>

    <button style="margin-top: 12px" @click="back">返回</button>
  </div>
</template>
