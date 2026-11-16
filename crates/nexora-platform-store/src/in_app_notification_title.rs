pub(crate) fn in_app_notification_title(kind: &str, spanish: bool) -> &'static str {
    let (es, en) = match kind {
        "knowledge_review" => (
            "Conocimiento pendiente de aprobación",
            "Knowledge approval requested",
        ),
        "knowledge_result" => (
            "Resultado de revisión de conocimiento",
            "Knowledge review decision",
        ),
        "assignment" => ("Tarea asignada", "Task assigned"),
        "task_updated" => ("Avance de una tarea", "Task progress updated"),
        "task_comment" => ("Nuevo comentario en una tarea", "New task comment"),
        "approval" => ("Solicitud de aprobación", "Approval requested"),
        "approval_result" => ("Resultado de aprobación", "Approval decision recorded"),
        "calendar_created" => ("Nuevo evento en el calendario", "New calendar event"),
        "calendar_updated" => ("Evento actualizado", "Calendar event updated"),
        "calendar_cancelled" => ("Evento cancelado", "Calendar event cancelled"),
        "calendar_attendance" => (
            "Respuesta de asistencia recibida",
            "Attendance response received",
        ),
        "leave_request" => ("Solicitud de ausencia", "Leave requested"),
        "leave_result" => (
            "Resultado de solicitud de ausencia",
            "Leave decision recorded",
        ),
        "overtime_request" => ("Solicitud de horas extra", "Overtime requested"),
        "overtime_result" => ("Resultado de horas extra", "Overtime decision recorded"),
        "holiday_request" => ("Solicitud de día festivo", "Holiday approval requested"),
        "holiday_result" => ("Resultado de día festivo", "Holiday decision recorded"),
        "feature_approval" => (
            "Mapa de funcionalidades pendiente",
            "Feature map approval requested",
        ),
        "feature_result" => (
            "Resultado del mapa de funcionalidades",
            "Feature map decision recorded",
        ),
        "workflow_assignment" => (
            "Asignación de flujo actualizada",
            "Workflow assignment updated",
        ),
        "role_changed" => ("Tu rol ha cambiado", "Your role has changed"),
        "access_request" => ("Nueva solicitud de acceso", "New access request"),
        "security_changed" => (
            "Configuración de tu cuenta actualizada",
            "Your account settings changed",
        ),
        "repository_failed" => (
            "Falló el procesamiento del repositorio",
            "Repository processing failed",
        ),
        "review_completed" => ("Revisión de código completada", "Code review completed"),
        "review_failed" => ("Falló la revisión de código", "Code review failed"),
        "review_stale" => ("La revisión necesita actualizarse", "Code review is stale"),
        _ => ("Notificación", "Notification"),
    };
    if spanish { es } else { en }
}
