use super::error::ArtifactAttributionError;
use super::model::ArtifactAttributionViolation;
use crate::core::types::Document;

impl Document {
    /// 按 14.8.2 校验一页上 artifact、真实内容与注解同逻辑结构的归属，
    /// 返回全部发现。
    ///
    /// 七条规则的判定见模块文档；`section`、`element`、`kid`、`annotation`
    /// 按规则填充，定位不到的层级为 `None`。页码越界或页面内容流损坏返回
    /// [`ArtifactAttributionError::MarkedContent`]；结构树损坏返回
    /// [`ArtifactAttributionError::Structure`]。父树或结构树**缺失**
    /// （而非损坏）不报错：规则 1 的 `element` 降级为 `None`，规则 2 与
    /// 注解规则 6/7 自然没有可评估的结构（结构树缺失时注解规则整组跳过，
    /// 见模块文档的生效范围）；`/MarkInfo` 读取失败只按「未声明 tag
    /// suspects」处理；注解读取失败只让规则 6/7 不产出发现。
    pub fn artifact_attribution_violations(
        &self,
        page_index: usize,
    ) -> Result<Vec<ArtifactAttributionViolation>, ArtifactAttributionError> {
        let sections = self.page_marked_content(page_index)?;
        let mut violations = Vec::new();
        // 父树与结构树是可选增强：缺失或损坏只影响 element 定位，不报错。
        let page_parents = self.page_struct_parents(page_index).ok().flatten();
        self.collect_artifact_identity_violations(
            page_index,
            &sections,
            &page_parents,
            &mut violations,
        );
        // 结构树整次诊断只取一次：损坏仍按原规则 2 报 Structure 错误，
        // 缺失（`Ok(None)`）时规则 2 与注解规则 6/7 不评估。
        let tree = self.structure_tree()?;
        self.collect_structure_attribution_violations(
            page_index,
            &sections,
            tree.as_ref(),
            &mut violations,
        );
        self.collect_unstructured_content_violations(page_index, &sections, &mut violations);
        self.collect_tag_suspect_violations(page_index, &sections, &mut violations);
        self.collect_annotation_attribution_violations(page_index, tree.as_ref(), &mut violations);
        Ok(violations)
    }
}
